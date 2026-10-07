# Official DTCG 2025.10 conformance corpus

This directory vendors the official Design Tokens Community Group test suite
for the 2025.10 Format and Resolver specifications.

| | |
|---|---|
| Upstream | <https://github.com/design-tokens/community-group> |
| Directory | `test-suite/tests/2025.10/` |
| Commit | `882ebd6716abef9a46d8ef5fb12e82009d6cee2e` (2026-09-08) |
| License | W3C 3-clause BSD License, see [`LICENSE`](LICENSE) |

`2025.10/` holds the top-level manifest, the `format` (220 fixtures) and
`resolver` (30 fixtures) manifests, and every fixture they list. The files are
unmodified; `.gitattributes` keeps them byte-identical on every platform.

`crates/dtcg_runtime/tests/official_test_suite.rs` evaluates every manifest
entry. Its `KNOWN_GAPS` list must equal the set of fixtures whose expectation
the runtime does not meet. The directory is excluded from the published crate
through `package.exclude`.

## Updating

An update is its own change:

1. Pick an upstream commit and export the tree byte-exact, for example:

   ```text
   git clone --filter=blob:none --no-checkout https://github.com/design-tokens/community-group cg
   git -C cg -c core.autocrlf=false archive <commit> test-suite/tests/2025.10 | tar -x
   ```

   Disabling `core.autocrlf` keeps the upstream LF line endings.
2. Replace `2025.10/` with the exported `test-suite/tests/2025.10/` tree.
   Never edit fixture files by hand. `git hash-object` of every file must
   equal the blob ids of `git -C cg ls-tree -r <commit> test-suite/tests/2025.10`.
3. Update the commit in this README and in `LICENSE`.
4. Run `cargo test -p dtcg_runtime --all-features --test official_test_suite`
   and re-derive `KNOWN_GAPS` from the reported mismatches.
