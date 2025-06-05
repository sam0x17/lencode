# Information for AI Agents

## Features

When running tests and lints, always run both with the `std` feature and without it, since
different code runs depending on whether `std` is enabled. For example:

```
cargo test --workspace && cargo test --workspace --features=std
```

```
cargo clippy --workspace --fix && cargo clippy --workspace --ix --features=std
```

etc.

## Lints & Tests

Always run the following after making changes. All must pass:

- `cargo test --workspace && cargo test --workspace --features=std`
- `cargo clippy --fix --workspace`
- `cargo clippy --fix --workspace --features=std`
- `cargo fix --workspace`
- `cargo fix --workspace --features=std`
- `cargo fmt`

## Coding Style

When writing unit tests I prefer to always have them inline at the bottom of the current file,
without an enclosing `mod tests {}`. When there are test-speciic imports simply put them above
the beginning of the tests section like so:

```rust

pub struct Something;

fn some_code {
  // ...
}

#[cfg(test)]
use something::for_tests::only;

#[cfg(test)]
use some::other::test::import;

#[test]
fn test_some_code() {
  some_code();
  // ...
}

#[test]
fn test_something() {
  something();
  // ...
}
```

We care a lot about performance and correctness.
