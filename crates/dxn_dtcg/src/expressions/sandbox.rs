//! The sandbox of a session: the counting allocator behind the memory and
//! stack limits, the step budget's interrupt handler, and the language
//! hardening that removes every ambient capability.
//!
//! Limits are enforced in Rust, never in JavaScript, and none can be caught
//! by expression code. QuickJS reports a refused allocation and a stack
//! overflow as ordinary exceptions a `catch` could swallow, so the allocator
//! records them in [`Guard`] instead: once a limit trips, the interrupt
//! handler stops the running evaluation at its next check (an exception
//! QuickJS makes uncatchable), and the evaluation fails afterwards even if
//! its code caught the original exception and returned.

use std::{cell::Cell, hint::black_box, rc::Rc};

use rquickjs::allocator::{Allocator, RustAllocator};

/// The limit an evaluation exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Trip {
    Memory,
    Stack,
    Steps,
}

/// The state the allocator and the interrupt handler share with the session.
pub(crate) struct Guard {
    memory_limit: usize,
    step_budget: usize,
    /// Stack bytes below [`Guard::stack_base`] past which an allocation
    /// counts as a stack overflow: the QuickJS stack limit less one large
    /// frame, because QuickJS throws its own overflow error (and allocates
    /// it) within one frame of that limit.
    stack_threshold: usize,
    used: Cell<usize>,
    /// The stack address where the running evaluation started; 0 while no
    /// evaluation runs.
    stack_base: Cell<usize>,
    ticks: Cell<usize>,
    /// A memory or stack trip; the session cannot be used after it.
    tripped: Cell<Option<Trip>>,
    /// Whether the running evaluation exceeded its step budget.
    steps_tripped: Cell<bool>,
    /// Whether the next interrupt check only restarts QuickJS's count.
    draining: Cell<bool>,
    /// Whether allocations count against the memory limit: not while the
    /// session starts, because QuickJS cannot survive a refused allocation
    /// while it creates a runtime. The session checks the total afterwards.
    enforcing: Cell<bool>,
}

impl Guard {
    pub(crate) fn new(memory_limit: usize, step_budget: usize, stack_limit: usize) -> Rc<Self> {
        Rc::new(Self {
            memory_limit,
            step_budget,
            stack_threshold: stack_limit - (stack_limit / 8).min(16 * 1024),
            used: Cell::new(0),
            stack_base: Cell::new(0),
            ticks: Cell::new(0),
            tripped: Cell::new(None),
            steps_tripped: Cell::new(false),
            draining: Cell::new(false),
            enforcing: Cell::new(false),
        })
    }

    /// Starts enforcing the memory limit once the session is set up; fails
    /// when the setup alone exceeds it.
    pub(crate) fn enforce(&self) -> Result<(), Trip> {
        self.enforcing.set(true);
        if self.used.get() > self.memory_limit {
            self.trip(Trip::Memory);
            return Err(Trip::Memory);
        }
        Ok(())
    }

    /// Starts the budget of one evaluation, whose frames lie below the
    /// caller's.
    pub(crate) fn begin(&self) {
        let marker = 0_u8;
        self.stack_base
            .set(std::ptr::from_ref(black_box(&marker)) as usize);
        self.ticks.set(0);
        self.steps_tripped.set(false);
    }

    /// Ends the running evaluation and returns the limit it exceeded.
    pub(crate) fn end(&self) -> Option<Trip> {
        self.stack_base.set(0);
        self.tripped
            .get()
            .or_else(|| self.steps_tripped.get().then_some(Trip::Steps))
    }

    /// A memory or stack trip so far: the session cannot be used after it.
    pub(crate) fn tripped(&self) -> Option<Trip> {
        self.tripped.get()
    }

    /// Makes the next interrupt check stop the running code without
    /// counting it: QuickJS restarts its count of calls and iterations
    /// before every check.
    pub(crate) fn drain(&self) {
        self.draining.set(true);
    }

    /// Records a stack overflow QuickJS reported as an exception.
    pub(crate) fn trip_stack(&self) {
        self.trip(Trip::Stack);
    }

    fn trip(&self, trip: Trip) {
        if self.tripped.get().is_none() {
            self.tripped.set(Some(trip));
        }
    }

    /// The interrupt handler: counts QuickJS's periodic checks against the
    /// step budget, and stops the evaluation once any limit tripped.
    pub(crate) fn interrupt_handler(self: &Rc<Self>) -> Box<dyn FnMut() -> bool> {
        let guard = Rc::clone(self);
        Box::new(move || {
            if guard.draining.replace(false) || guard.tripped.get().is_some() {
                return true;
            }
            if guard.stack_base.get() == 0 {
                return false;
            }
            let ticks = guard.ticks.get() + 1;
            guard.ticks.set(ticks);
            if ticks > guard.step_budget {
                guard.steps_tripped.set(true);
                return true;
            }
            false
        })
    }

    /// Whether `size` more bytes fit; records a refusal and an allocation
    /// near the stack limit.
    fn admit(&self, size: usize) -> bool {
        let base = self.stack_base.get();
        if base != 0 {
            let marker = 0_u8;
            let here = std::ptr::from_ref(black_box(&marker)) as usize;
            if base.saturating_sub(here) > self.stack_threshold {
                self.trip(Trip::Stack);
            }
        }
        if self.enforcing.get() && self.used.get().saturating_add(size) > self.memory_limit {
            self.trip(Trip::Memory);
            return false;
        }
        true
    }
}

/// QuickJS's allocator: Rust's global allocator, counting what it hands out
/// against the memory limit.
pub(crate) struct CountingAllocator {
    guard: Rc<Guard>,
    inner: RustAllocator,
}

impl CountingAllocator {
    pub(crate) fn new(guard: Rc<Guard>) -> Self {
        Self {
            guard,
            inner: RustAllocator,
        }
    }

    fn counted(&self, ptr: *mut u8) -> *mut u8 {
        if !ptr.is_null() {
            // SAFETY: an allocation of the inner allocator, just made.
            let size = unsafe { RustAllocator::usable_size(ptr) };
            self.guard.used.set(self.guard.used.get() + size);
        }
        ptr
    }
}

// SAFETY: every allocation comes from `RustAllocator`, which meets the
// trait's requirements; this only refuses some (returning null, as allowed)
// and counts the sizes it reports.
unsafe impl Allocator for CountingAllocator {
    fn alloc(&mut self, size: usize) -> *mut u8 {
        if !self.guard.admit(size) {
            return std::ptr::null_mut();
        }
        let ptr = self.inner.alloc(size);
        self.counted(ptr)
    }

    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
        let Some(total) = count.checked_mul(size) else {
            return std::ptr::null_mut();
        };
        if !self.guard.admit(total) {
            return std::ptr::null_mut();
        }
        let ptr = self.inner.calloc(count, size);
        self.counted(ptr)
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        // SAFETY: the caller hands back an allocation of this allocator.
        unsafe {
            let size = RustAllocator::usable_size(ptr);
            self.guard.used.set(self.guard.used.get() - size);
            self.inner.dealloc(ptr);
        }
    }

    unsafe fn realloc(&mut self, ptr: *mut u8, new_size: usize) -> *mut u8 {
        // SAFETY: the caller hands over an allocation of this allocator.
        let old = unsafe { RustAllocator::usable_size(ptr) };
        if !self.guard.admit(new_size.saturating_sub(old)) {
            return std::ptr::null_mut();
        }
        // SAFETY: as above; on failure the old allocation stays.
        let new = unsafe { self.inner.realloc(ptr, new_size) };
        if !new.is_null() {
            self.guard.used.set(self.guard.used.get() - old);
        }
        self.counted(new)
    }

    unsafe fn usable_size(ptr: *mut u8) -> usize {
        // SAFETY: the caller hands over an allocation of this allocator.
        unsafe { RustAllocator::usable_size(ptr) }
    }
}

/// Removes every ambient capability before any expression or module runs:
/// `eval` and the function constructors (no code from strings),
/// `Math.random` (no randomness), the `Promise` constructor (no promises,
/// though async functions still make ones that never settle), the error
/// stack hooks (`Error.prepareStackTrace`, `Error.stackTraceLimit`), and
/// every mutable shared object: the built-in prototypes, constructors and
/// namespaces reachable from `globalThis` or from the values that syntax
/// alone creates (iterators, generators, async functions, promises), and
/// `globalThis` itself, are frozen. Members that code commonly assigns on
/// its own objects (`constructor`, `toString`, an error's `name`, …) become
/// accessors whose setter defines the property on the object assigned to, so
/// freezing a prototype does not break `Sub.prototype.constructor = Sub`
/// (the "override mistake"); their original values are frozen too.
///
/// `Date`, `Proxy`, `WeakRef` and timers are never installed, and
/// `queueMicrotask` is removed. Returns the deep-freeze function inputs pass
/// through and the function that runs until QuickJS's next interrupt check.
pub(crate) const HARDEN: &str = r#"
(() => {
  "use strict";
  const deny = (label) => function () {
    throw new TypeError(label + " is not available in expressions");
  };
  const replace = (fn, value) => {
    Object.defineProperty(Object.getPrototypeOf(fn), "constructor", {
      value, writable: false, enumerable: false, configurable: false,
    });
  };
  const blocked = deny("the Function constructor");
  blocked.prototype = Function.prototype;
  replace(function () {}, blocked);
  replace(async function () {}, deny("the AsyncFunction constructor"));
  replace(function* () {}, deny("the GeneratorFunction constructor"));
  replace(async function* () {}, deny("the AsyncGeneratorFunction constructor"));
  const promise = (async () => {})();
  replace(promise, deny("Promise"));
  Object.defineProperty(globalThis, "Function", { value: blocked, writable: false, configurable: false });
  delete globalThis.eval;
  delete globalThis.queueMicrotask;
  Object.defineProperty(Math, "random", { value: deny("Math.random"), writable: false, configurable: false });
  // The stack hooks are accessors on `Error` that write engine state.
  for (const name of ["prepareStackTrace", "stackTraceLimit"]) {
    Object.defineProperty(Error, name, { value: undefined, writable: false, configurable: false });
  }

  const tamed = [
    "constructor", "toString", "toLocaleString", "valueOf", "hasOwnProperty", "isPrototypeOf",
    "propertyIsEnumerable", "toJSON", "name", "message",
  ];
  const pending = [];
  const visit = (value) => {
    if ((typeof value === "object" && value !== null) || typeof value === "function") pending.push(value);
  };
  const tame = (proto) => {
    for (const name of tamed) {
      const d = Object.getOwnPropertyDescriptor(proto, name);
      if (!d || !("value" in d) || !d.writable) continue;
      const value = d.value;
      visit(value);
      Object.defineProperty(proto, name, {
        get() { return value; },
        set(v) {
          if (this === proto) throw new TypeError(`Cannot assign to read only property '${name}'`);
          Object.defineProperty(this, name, { value: v, writable: true, enumerable: true, configurable: true });
        },
        enumerable: d.enumerable,
        configurable: false,
      });
    }
  };
  const iteratorOf = (value) => Object.getPrototypeOf(value[Symbol.iterator]());
  const roots = [
    globalThis,
    promise,
    Object.getPrototypeOf(function* () {}),
    Object.getPrototypeOf(async function () {}),
    Object.getPrototypeOf(async function* () {}),
    Object.getPrototypeOf((function* () {})()),
    Object.getPrototypeOf((async function* () {})()),
    iteratorOf([]),
    iteratorOf(""),
    iteratorOf(new Map()),
    iteratorOf(new Set()),
    Object.getPrototypeOf(iteratorOf([])),
    Object.getPrototypeOf("a".matchAll(/a/g)),
    Object.getPrototypeOf([].values().map((x) => x)),
    Object.getPrototypeOf(Iterator.from({ next() { return { done: true }; } })),
  ];
  roots.forEach(visit);
  const seen = new Set();
  while (pending.length > 0) {
    const value = pending.pop();
    if (seen.has(value)) continue;
    seen.add(value);
    if (value !== globalThis && Object.prototype.hasOwnProperty.call(value, "constructor")) tame(value);
    for (const key of Reflect.ownKeys(value)) {
      const d = Object.getOwnPropertyDescriptor(value, key);
      if ("value" in d) visit(d.value);
      else { visit(d.get); visit(d.set); }
    }
    visit(Object.getPrototypeOf(value));
  }
  for (const value of seen) Object.freeze(value);

  const freeze = Object.freeze;
  const isFrozen = Object.isFrozen;
  const values = Object.values;
  const deepFreeze = (value) => {
    if (typeof value === "object" && value !== null && !isFrozen(value)) {
      freeze(value);
      for (const item of values(value)) deepFreeze(item);
    }
    return value;
  };
  const drain = () => { for (;;) {} };
  return [deepFreeze, drain];
})()
"#;
