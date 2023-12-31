use std::{cell::RefCell, rc::Rc};

use crate::optimizer::CachedImage;

/// Extracts all the images from all non-dynamic <Route/>s in the given Leptos App.
#[cfg(feature = "ssr")]
pub fn find_app_images<IV>(app_fn: impl Fn(leptos::Scope) -> IV + 'static) -> Vec<CachedImage>
where
    IV: leptos::IntoView + 'static,
{
    find_app_images_with_mount(app_fn, || (), || ())
}

/// Extracts all the images from all non-dynamic <Route/>s in the given Leptos App.
#[cfg(feature = "ssr")]
pub fn find_app_images_with_mount<IV>(
    app_fn: impl Fn(leptos::Scope) -> IV + 'static,
    before_mount: impl Fn() + 'static,
    after_mount: impl Fn() + 'static,
) -> Vec<CachedImage>
where
    IV: leptos::IntoView + 'static,
{
    let app_fn = Rc::new(app_fn);

    let app = {
        let app_fn = app_fn.clone();
        move |cx: leptos::Scope| app_fn(cx)
    };

    let routes = leptos_router::generate_route_list_inner(app);
    let paths: Vec<String> = routes
        .into_iter()
        .map(|route| route.path().to_string())
        .collect();

    let app = {
        let app_fn = app_fn.clone();
        move |cx: leptos::Scope| app_fn(cx)
    };

    find_app_images_from_paths(app, paths, before_mount, after_mount)
}

/// Context to contain all possible images.
#[derive(Clone, Default, Debug)]
pub(crate) struct IntrospectImageContext(pub(crate) Rc<RefCell<Vec<CachedImage>>>);

/// Extracts the CachedImages used in the provided paths.
#[cfg(feature = "ssr")]
pub fn find_app_images_from_paths<IV, P>(
    app_fn: impl Fn(leptos::Scope) -> IV + 'static,
    paths: P,
    before_mount: impl Fn() + 'static,
    after_mount: impl Fn() + 'static,
) -> Vec<CachedImage>
where
    P: IntoIterator<Item = String>,
    IV: leptos::IntoView + 'static,
{
    use leptos::*;

    let runtime = leptos::create_runtime();

    let app_fn = Rc::new(app_fn);
    let before_mount = Rc::new(before_mount);
    let after_mount = Rc::new(after_mount);

    let images = paths
        .into_iter()
        .map(|path| format!("http://leptos.dev{}", path))
        .map(|path| {
            run_scope(runtime, {
                let app_fn = app_fn.clone();
                let before_mount = before_mount.clone();
                let after_mount = after_mount.clone();
                move |cx| {
                    let integration = leptos_router::ServerIntegration { path };

                    provide_context(
                        cx,
                        leptos_router::RouterIntegrationContext::new(integration),
                    );

                    let context = IntrospectImageContext::default();
                    provide_context(cx, context.clone());

                    before_mount();
                    leptos::suppress_resource_load(true);
                    _ = app_fn(cx).into_view(cx);
                    leptos::suppress_resource_load(false);
                    after_mount();

                    let images = context.0.borrow();
                    images.clone()
                }
            })
        })
        .flatten()
        .collect();

    runtime.dispose();

    images
}
