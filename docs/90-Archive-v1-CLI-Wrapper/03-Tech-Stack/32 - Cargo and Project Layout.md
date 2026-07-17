---
tags: [tech, reference]
---

# 32 - Cargo and Project Layout

Cargo is Rust's build tool and package manager. It's how you create, build, run, and
add dependencies to the project.

## Commands you'll live in

```
cargo new fuxx-cli      # create the project
cargo run -- claude     # build & run; args after -- go to your program
cargo build             # compile (debug)
cargo build --release   # compile optimized binary for distribution
cargo add clap          # add a dependency
```

## Expected layout (v1)

```
fuxx-cli/
├── Cargo.toml          # project metadata + dependency list
├── Cargo.lock          # exact resolved versions (commit this)
└── src/
    └── main.rs         # entry point; grows into modules as needed
```

As the code grows, split `main.rs` into modules matching the architecture:
`process.rs` (wrapper), `detect.rs` (OSC scanning), `notify.rs` (dispatch). Mirrors
[[20 - Architecture Overview]].

## Related

- [[31 - Rust Language Choice]]
- [[33 - Key Crates]]
- [[35 - Repo and Git]]
