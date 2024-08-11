#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::Cell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

struct IterSinkInner<T> {
    value: Cell<Option<T>>,
}

impl<T> IterSinkInner<T> {
    fn new() -> Self {
        Self {
            value: Cell::new(None),
        }
    }

    fn set_value(&self, value: T) {
        self.value.set(Some(value));
    }

    fn take_value(&self) -> Option<T> {
        self.value.take()
    }
}

pub struct IterSink<T> {
    inner: Rc<IterSinkInner<T>>,
}

impl<T> IterSink<T> {
    fn new(inner: Rc<IterSinkInner<T>>) -> Self {
        Self { inner }
    }

    pub fn yield_value(&self, value: T) -> impl Future<Output = ()> {
        self.inner.set_value(value);
        IterYield::new()
    }
}

struct IterYield {
    suspended: bool,
}

impl IterYield {
    fn new() -> Self {
        Self { suspended: false }
    }
}

impl Future for IterYield {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.suspended {
            Poll::Ready(())
        } else {
            self.get_mut().suspended = true;
            Poll::Pending
        }
    }
}

fn make_noop_raw_waker() -> RawWaker {
    fn noop(_: *const ()) {}

    fn noop_clone(_: *const ()) -> RawWaker {
        make_noop_raw_waker()
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);
    RawWaker::new(core::ptr::null(), &VTABLE)
}

fn make_noop_waker() -> Waker {
    let raw_waker = make_noop_raw_waker();

    // SAFETY: the no-op waker is unconditionally safe.
    unsafe { Waker::from_raw(raw_waker) }
}

struct IterAdapter<T, Fut> {
    iter_sink_inner: Rc<IterSinkInner<T>>,
    future: Pin<Box<Fut>>,
}

impl<T, Fut> IterAdapter<T, Fut> {
    fn new<IntoFut>(into_future: IntoFut) -> Self
    where
        IntoFut: FnOnce(IterSink<T>) -> Fut,
    {
        let iter_sink_inner = Rc::new(IterSinkInner::new());
        let iter_sink = IterSink::new(Rc::clone(&iter_sink_inner));
        let future = Box::pin(into_future(iter_sink));
        Self { iter_sink_inner, future }
    }
}

impl<T, Fut> Iterator for IterAdapter<T, Fut>
where
    Fut: Future<Output = ()>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let waker = make_noop_waker();
        let mut cx = Context::from_waker(&waker);
        match self.future.as_mut().poll(&mut cx) {
            Poll::Pending => Some(
                self.iter_sink_inner
                    .take_value()
                    .expect("Future did not yield a value"),
            ),
            Poll::Ready(()) => None,
        }
    }
}

pub fn make_iter<T, Fut, IntoFut>(into_future: IntoFut) -> impl Iterator<Item = T>
where
    Fut: Future<Output = ()>,
    IntoFut: FnOnce(IterSink<T>) -> Fut,
{
    IterAdapter::new(into_future)
}

#[cfg(test)]
mod tests {
    use super::{make_iter, IterSink};
    use alloc::string::String;
    use alloc::vec::Vec;

    async fn count_up_impl(sink: IterSink<u64>, start: u64) {
        for i in start.. {
            sink.yield_value(i).await;
        }
    }

    fn count_up(start: u64) -> impl Iterator<Item = u64> {
        make_iter(move |sink| count_up_impl(sink, start))
    }

    #[test]
    fn test_count_up() {
        let mut iter = count_up(5);
        assert_eq!(iter.next(), Some(5));
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.next(), Some(7));
        assert_eq!(iter.next(), Some(8));
        assert_eq!(iter.next(), Some(9));
        assert_eq!(iter.next(), Some(10));
        assert_eq!(iter.next(), Some(11));
        assert_eq!(iter.next(), Some(12));
    }

    async fn join_to_strings_impl(
        sink: IterSink<String>,
        chars: impl IntoIterator<Item = char>,
    ) {
        let mut is_whitespace = true;
        let mut current_string = String::new();
        for c in chars {
            match (c.is_whitespace(), is_whitespace) {
                (true, true) => (),
                (true, false) => {
                    sink.yield_value(core::mem::take(&mut current_string)).await;
                    is_whitespace = true;
                }
                (false, _) => {
                    current_string.push(c);
                    is_whitespace = false;
                }
            }
        }

        if !is_whitespace {
            sink.yield_value(current_string).await;
        }
    }

    fn join_to_strings(chars: impl IntoIterator<Item = char>) -> impl Iterator<Item = String> {
        make_iter(move |sink| join_to_strings_impl(sink, chars))
    }

    #[test]
    fn test_join_to_strings() {
        let input1 = "Hello   world!";
        let output1 = join_to_strings(input1.chars()).collect::<Vec<_>>();
        assert_eq!(&output1, &[String::from("Hello"), String::from("world!")],);

        let input2 = "    test\ton more\n  data ";
        let output2 = join_to_strings(input2.chars()).collect::<Vec<_>>();
        assert_eq!(
            &output2,
            &[
                String::from("test"),
                String::from("on"),
                String::from("more"),
                String::from("data"),
            ],
        );
    }
}
