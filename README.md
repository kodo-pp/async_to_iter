# Convert async functions to generators on stable Rust

This crate allows to write generator-like `async` code to implement `Iterator`
on today's (August 2024) stable Rust.

## License
This crate is dual-licensed under the terms of both [Apache 2.0](LICENSE-APACHE) and [MIT](LICENSE-MIT)
licenses.

## Usage example

```rust
use async_to_iter::{IterSink, make_iter};

// Async code that implements the iterator.
async fn count_to_impl(sink: IterSink<u32>, n: u32) {
    for i in 1..=n {
        sink.yield_value(i).await;
    }
}

// Function that constructs the iterator from async code using `make_iter()`.
fn count_to(n: u32) -> impl Iterator<Item = u32> {
    make_iter(move |sink| count_to_impl(sink, n))
}

fn main() {
    // The resulting iterator can be accessed as usual.
    let mut iter = count_to(3);
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(2));
    assert_eq!(iter.next(), Some(3));
    assert_eq!(iter.next(), None);
}
```

## FAQ

### How does this work?
The compiler automatically converts async code to a state machine that saves its internal
state across `await` points. This is how `async` code has long been implemented in Rust.
`make_iter` returns a type implementing `Iterator` that translates `Iterator::next()` to `Future::poll()` calls.

Yielding value and suspending the future in yield points is implemented by `IterSink::yield_value()`.
It saves the value provided by async code (it will later be returned from `Iterator::next()`)
and returns a `Future` that becomes ready the *second* time `Future::poll` is called.
This way, execution of async code pauses exactly once each time a value is yielded.

### Does this crate use `unsafe`?
It uses it just for one thing: to create a no-op [`Waker`](https://doc.rust-lang.org/nightly/core/task/struct.Waker.html).
It is safe because a no-op waker does nothing, and it actually re-implements an unstable *safe* functions
from the standard library: [`Waker::noop()`](https://doc.rust-lang.org/nightly/core/task/struct.Waker.html#method.noop).
The rest of this crate only uses safe Rust code.

### Is this a zero-cost abstraction?
Unfortunately, no. The future that yields iterator output values is stored on heap — this is necessary to
pin it without affecting the usability of the `Iterator` implementation returned from `make_iter()`.
It also makes some other allocations now. The number of allocations may be optimized in the future,
but it is unlikely that this will become a zero-cost abstraction.

If you need zero-cost generators in Rust, you will likely have to use some Nightly features.

### Does this work in `#[no_std]` environments?
Yes, this crate is `#[no_std]`. However, it needs the `alloc` crate and a global allocator.
