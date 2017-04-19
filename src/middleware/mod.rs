//! Defines types for Gotham middleware

use std::io;
use handler::HandlerFuture;
use state::State;
use hyper::server::Request;

pub mod pipeline;

/// `Middleware` has the opportunity to provide additional behaviour to the request / response
/// interaction. Middleware-specific state data can be recorded in the [`State`][State] struct for
/// use elsewhere.
///
/// [State]: ../state/struct.State.html
///
/// # Examples
///
/// Taking no action, and immediately passing the request through to the rest of the application:
///
/// ```rust,no_run
/// # extern crate gotham;
/// # extern crate hyper;
/// #
/// # use gotham::handler::{Handler, HandlerFuture};
/// # use gotham::middleware::Middleware;
/// # use gotham::state::State;
/// # use hyper::server::{Request, Response};
/// #
/// struct NoopMiddleware;
///
/// impl Middleware for NoopMiddleware {
///     fn call<Chain>(&self, state: State, req: Request, chain: Chain) -> Box<HandlerFuture>
///         where Chain: FnOnce(State, Request) -> Box<HandlerFuture> + Send + 'static
///     {
///         chain(state, req)
///     }
/// }
/// #
/// # fn main() {}
/// ```
///
/// Recording a piece of state data before passing the request through:
///
/// ```rust,no_run
/// # extern crate gotham;
/// # extern crate hyper;
/// #
/// # use gotham::handler::{Handler, HandlerFuture};
/// # use gotham::middleware::Middleware;
/// # use gotham::state::{State, StateData};
/// # use hyper::server::{Request, Response};
/// #
/// struct MiddlewareWithStateData;
///
/// struct MiddlewareStateData {
///     i: i32,
/// }
///
/// impl StateData for MiddlewareStateData {}
///
/// impl Middleware for MiddlewareWithStateData {
///     fn call<Chain>(&self, mut state: State, req: Request, chain: Chain) -> Box<HandlerFuture>
///         where Chain: FnOnce(State, Request) -> Box<HandlerFuture> + Send + 'static
///     {
///         state.put(MiddlewareStateData { i: 10 });
///         chain(state, req)
///     }
/// }
/// #
/// # fn main() {}
/// ```
///
/// Terminating the request early based on some arbitrary condition:
///
/// ```rust,no_run
/// # extern crate gotham;
/// # extern crate hyper;
/// # extern crate futures;
/// #
/// # use gotham::handler::{Handler, HandlerFuture};
/// # use gotham::middleware::Middleware;
/// # use gotham::state::{State, StateData};
/// # use hyper::server::{Request, Response};
/// # use hyper::{Method, StatusCode};
/// # use futures::{future, Future};
/// #
/// struct ConditionalMiddleware;
///
/// impl Middleware for ConditionalMiddleware {
///     fn call<Chain>(&self, state: State, req: Request, chain: Chain) -> Box<HandlerFuture>
///         where Chain: FnOnce(State, Request) -> Box<HandlerFuture> + Send + 'static
///     {
///         if *req.method() == Method::Get {
///             chain(state, req)
///         } else {
///             let response = Response::new().with_status(StatusCode::MethodNotAllowed);
///             future::ok((state, response)).boxed()
///         }
///     }
/// }
/// #
/// # fn main() {}
/// ```
///
/// Asynchronous middleware, which continues the request after some action completes:
///
/// ```rust,no_run
/// # extern crate gotham;
/// # extern crate hyper;
/// # extern crate futures;
/// #
/// # use gotham::handler::{Handler, HandlerFuture};
/// # use gotham::middleware::Middleware;
/// # use gotham::state::State;
/// # use hyper::server::{Request, Response};
/// # use futures::{future, Future};
/// #
/// struct AsyncMiddleware;
///
/// impl Middleware for AsyncMiddleware {
///     fn call<Chain>(&self, state: State, req: Request, chain: Chain) -> Box<HandlerFuture>
///         where Chain: FnOnce(State, Request) -> Box<HandlerFuture> + Send + 'static
///     {
///         // This could be any asynchronous action. `future::lazy(_)` defers a function until the
///         // next cycle of tokio's event loop.
///         let f = future::lazy(|| future::ok(()));
///         f.and_then(move |_| chain(state, req)).boxed()
///     }
/// }
/// #
/// # fn main() {}
/// ```
pub trait Middleware {
    /// Entry point to the middleware. To pass the request on to the application, the middleware
    /// invokes the `chain` function with the provided `state` and `request`.
    ///
    /// By convention, the middleware should:
    ///
    /// * Avoid modifying the `Request`, unless it is already determined that the response will be
    ///   generated by the middleware (i.e. without calling `chain`);
    /// * Ensure to pass the same `State` to `chain`, rather than creating a new `State`.
    fn call<Chain>(&self, state: State, request: Request, chain: Chain) -> Box<HandlerFuture>
        where Chain: FnOnce(State, Request) -> Box<HandlerFuture> + Send + 'static,
              Self: Sized;
}

/// Creates new `Middleware` values.
pub trait NewMiddleware {
    /// The type of `Middleware` created by the implementor.
    type Instance: Middleware;

    /// Create and return a new `Middleware` value.
    fn new_middleware(&self) -> io::Result<Self::Instance>;
}
