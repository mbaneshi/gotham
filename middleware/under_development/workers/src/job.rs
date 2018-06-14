use futures::{Future, IntoFuture};

use gotham::state::State;

use pool;

/// A job which can be executed on a thread pool after being prepared.
///
/// The `Job::prepare` function is called on one of the main threads, and so it **must not** block.
/// Since the `State` cannot be sent between threads, it is available in preparing a job for
/// execution, and is returned when the job completes.
pub trait Job {
    type Item: Send + 'static;
    type Error: Send + 'static;

    type Prepared: PreparedJob<Item = Self::Item, Error = Self::Error> + Send + 'static;

    fn prepare(self, &mut State) -> Self::Prepared;
}

/// A `PreparedJob` is created from a `Job` and then executed on the workers pool. There is
/// typically no need to consume this type directly, unless it is being implemented for a custom
/// job type.
///
/// As this is run on a workers pool and not in the event loop, it is appropriate for synchronous
/// I/O and other blocking actions to be performed.
pub trait PreparedJob {
    type Item: Send + 'static;
    type Error: Send + 'static;
    type Future: Future<Item = Self::Item, Error = Self::Error> + Send + 'static;

    type Output: IntoFuture<Future = Self::Future, Item = Self::Item, Error = Self::Error>;

    fn run(self) -> Self::Output;
}

/// The type returned after executing a job of type `J`. As the worker takes ownership of the
/// `State` it must return that ownership when the future completes.
pub type WorkerFuture<J> =
    Future<Item = (State, <J as Job>::Item), Error = (State, <J as Job>::Error)>;

/// Runs the given job on the worker pool.
///
/// This function will panic if the middleware has not added the pool to `State`.
pub fn run_with_worker<J>(mut state: State, job: J) -> Box<WorkerFuture<J>>
where
    J: Job,
{
    let prepared_job = job.prepare(&mut state);

    let f = pool::run_in_thread_pool(state, || prepared_job.run());
    Box::new(f)
}

impl<F, E, P, T> Job for F
where
    F: FnOnce(&mut State) -> P + Send + 'static,
    P: PreparedJob<Item = T, Error = E> + Send + 'static,
    E: Send + 'static,
    T: Send + 'static,
{
    type Item = T;
    type Error = E;
    type Prepared = P;

    fn prepare(self, state: &mut State) -> Self::Prepared {
        self(state)
    }
}

impl<F, R, E, T> PreparedJob for F
where
    F: FnOnce() -> R + Send + 'static,
    R: IntoFuture<Item = T, Error = E> + 'static,
    R::Future: Send + 'static,
    E: Send + 'static,
    T: Send + 'static,
{
    type Item = T;
    type Error = E;
    type Future = R::Future;
    type Output = R;

    fn run(self) -> Self::Output {
        self()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::future::FutureResult;
    use futures_cpupool::CpuPool;
    use gotham::handler::{HandlerFuture, IntoHandlerError};
    use gotham::helpers::http::response::create_response;
    use gotham::test::TestServer;
    use hyper::StatusCode;
    use mime;
    use std::io;
    use std::sync::{Arc, Mutex};

    use pool::WorkersPool;

    #[derive(StateData, Clone)]
    struct ThreadSafeValue {
        n: Arc<Mutex<usize>>,
    }

    struct TestJob;

    struct PreparedTestJob {
        v: ThreadSafeValue,
    }

    impl Job for TestJob {
        type Item = ();
        type Error = ();
        type Prepared = PreparedTestJob;

        fn prepare(self, state: &mut State) -> Self::Prepared {
            let v = state.borrow::<ThreadSafeValue>().clone();
            PreparedTestJob { v }
        }
    }

    impl PreparedJob for PreparedTestJob {
        type Item = ();
        type Error = ();
        type Future = FutureResult<Self::Item, Self::Error>;
        type Output = Self::Future;

        fn run(self) -> Self::Output {
            *(self.v.n.lock().unwrap()) += 1;
            Ok(()).into()
        }
    }

    #[test]
    fn run_with_worker_tests() {
        fn handler(mut state: State) -> Box<HandlerFuture> {
            // Simulate the job of the middleware.
            state.put(WorkersPool {
                pool: CpuPool::new(1),
            });

            state.put(ThreadSafeValue {
                n: Arc::new(Mutex::new(41)),
            });

            let f = run_with_worker(state, TestJob).then(|r| {
                let (state, t) = r.unwrap_or_else(|_| panic!("not ok"));
                let response = create_response(
                    &state,
                    StatusCode::Ok,
                    Some((
                        format!("{}", *(state.borrow::<ThreadSafeValue>().n.lock().unwrap()))
                            .into_bytes(),
                        mime::TEXT_PLAIN,
                    )),
                );
                Ok((state, response))
            });

            Box::new(f)
        }

        let test_server = TestServer::new(|| Ok(handler)).unwrap();
        let client = test_server.client();
        let response = client.get("https://example.com/").perform().unwrap();
        assert_eq!(response.status(), StatusCode::Ok);
        let body = response.read_utf8_body().unwrap();
        assert_eq!(&body, "42");
    }

    #[test]
    fn run_with_worker_closure_tests() {
        fn handler(mut state: State) -> Box<HandlerFuture> {
            // Simulate the job of the middleware.
            state.put(WorkersPool {
                pool: CpuPool::new(1),
            });

            let f = run_with_worker(state, |_state: &mut State| {
                let x = 41;
                move || Ok(x + 1)
            }).then(|r: Result<(State, usize), (State, ())>| {
                let (state, t) = r.unwrap_or_else(|_| panic!("not ok"));
                let response = create_response(
                    &state,
                    StatusCode::Ok,
                    Some((format!("{}", t).into_bytes(), mime::TEXT_PLAIN)),
                );
                Ok((state, response))
            });

            Box::new(f)
        }

        let test_server = TestServer::new(|| Ok(handler)).unwrap();
        let client = test_server.client();
        let response = client.get("https://example.com/").perform().unwrap();
        assert_eq!(response.status(), StatusCode::Ok);
        let body = response.read_utf8_body().unwrap();
        assert_eq!(&body, "42");
    }
}
