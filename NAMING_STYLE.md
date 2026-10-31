# naming style

rust code in this repo uses plain rust naming, no type prefixes:

- types, traits, enum variants: `PascalCase`
- functions, methods, variables, modules: `snake_case`
- consts and statics: `UPPER_SNAKE_CASE`

why not the `b_`/`sz_`/`str_` prefixes used in the C/C++ projects: the
compiler already knows and checks every type, rust-analyzer shows it
inline, and the prefixes would clash with every crate and every rust
reader.

## enforcement

`Cargo.toml` denies `non_snake_case`, `non_camel_case_types` and
`non_upper_case_globals` under `[lints.rust]`, so any `cargo build`,
`check` or `clippy` fails on a violation, pointing at the line and the
expected spelling. CI runs it as the `naming-lint` step; the pre-commit
hook runs clippy, which includes it.
