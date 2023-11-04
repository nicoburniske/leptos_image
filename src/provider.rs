use crate::optimizer::CachedImage;
use lazy_static::lazy_static;
use leptos::*;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

/// Provides Image Cache Context to the given scope.
/// This should go in the base of your Leptos <App/>.
///
///Example
///
/// ```
/// use leptos_image::*;
///
/// #[component]
/// pub fn App(cx: Scope) -> impl IntoView {
///     provide_image_context(cx);
///
///     view! { cx,
///
///     }
/// }
///
/// ```
pub fn provide_image_context() {
    let resource: ImageResource = create_blocking_resource(
        || (),
        |_| async {
            IMAGE_CACHE
                .read()
                .map(|c| c.clone())
                .map(|c| c.into_iter().collect::<Vec<_>>())
                .unwrap_or(vec![])
        },
    );

    leptos::provide_context(resource);
}

type ImageResource = Resource<(), Vec<(CachedImage, String)>>;

pub(crate) fn use_image_cache_resource() -> ImageResource {
    use_context::<ImageResource>().expect("Missing Image Resource")
}

#[cfg(feature = "ssr")]
pub(crate) fn add_image_cache<I>(images: I)
where
    I: IntoIterator<Item = (CachedImage, String)>,
{
    let mut cache = IMAGE_CACHE.write().expect("Failed to lock image cache");
    for (image, svg) in images.into_iter() {
        cache.insert(image, svg);
    }
}

lazy_static! {
    // CacheImage -> Blur Image SVG data (literally the svg data, not a file_path).
    pub(crate) static ref IMAGE_CACHE: Arc<RwLock<HashMap<CachedImage, String>>> =
        Arc::new(RwLock::new(HashMap::new()));
}
