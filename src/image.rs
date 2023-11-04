use crate::optimizer::*;

use leptos::{logging::debug_warn, *};
use leptos_meta::Link;

/**
 */

/// Image component for rendering optimized static images.
/// Images MUST be static. Will not work with dynamic images.
#[component]
pub fn Image(
    /// Image source. Should be path relative to root.
    #[prop(into)]
    src: String,
    /// Resize image height, but will still maintain the same aspect ratio.
    height: u32,
    /// Resize image width, but will still maintain the same aspect ratio.
    width: u32,
    /// Image quality. 0-100.
    #[prop(default = 75_u8)]
    quality: u8,
    /// Will add blur image to head if true.
    #[prop(default = false)]
    blur: bool,
    /// Will add preload link to head if true.
    #[prop(default = false)]
    priority: bool,
    /// Lazy load image.
    #[prop(default = true)]
    lazy: bool,
    /// Image alt text.
    #[prop(into)]
    alt: String,
    /// Style class for image.
    #[prop(into, optional)]
    class: Option<AttributeValue>,
) -> impl IntoView {
    if src.starts_with("http") {
        debug_warn!("Image component only supports static images.");
        let loading = if lazy { "lazy" } else { "eager" };
        return view! {  <img src=src alt=alt class=class loading=loading/> }.into_view();
    }

    let blur_image = {
        CachedImage {
            src: src.clone(),
            option: CachedImageOption::Blur(Blur {
                width: 20,
                height: 20,
                svg_width: 100,
                svg_height: 100,
                sigma: 15,
            }),
        }
    };

    let opt_image = {
        CachedImage {
            src: src.clone(),
            option: CachedImageOption::Resize(Resize {
                quality,
                width,
                height,
            }),
        }
    };

    // Load images into context for blur generation.
    // Happens on server start.
    #[cfg(feature = "ssr")]
    if let Some(context) = use_context::<crate::introspect::IntrospectImageContext>() {
        let mut images = context.0.borrow_mut();
        images.push(opt_image.clone());
        if blur {
            images.push(blur_image.clone());
        }
    }

    let opt_image = opt_image.get_url_encoded();

    if blur {
        // Retrieve value from Cache if it exists. Doing this per-image to allow image introspection.
        let resource = crate::use_image_cache_resource();

        let blur_image = store_value(blur_image);
        let opt_image = store_value(opt_image);
        let alt = store_value(alt);
        let class = store_value(class.map(|c| c.into_attribute_boxed()));

        view! {
            <Suspense fallback=|| ()>
                {move || {
                    resource
                        .get()
                        .map(|images| {
                            let placeholder_svg = images
                                .iter()
                                .find(|(c, _)| blur_image.with_value(|b| b == c))
                                .map(|c| c.1.clone());
                            let svg = {
                                if let Some(svg_data) = placeholder_svg {
                                    SvgImage::InMemory(svg_data)
                                } else {
                                    SvgImage::Request(blur_image.get_value().get_url_encoded())
                                }
                            };
                            let opt_image = opt_image.get_value();
                            let class = class.get_value();
                            let alt = alt.get_value();
                            view! {  <CacheImage lazy svg opt_image alt class=class priority/> }
                                .into_view()
                        })
                }}
            </Suspense>
        }
    } else {
        let loading = if lazy { "lazy" } else { "eager" };
        view! {  <img alt=alt class=class decoding="async" loading=loading src=opt_image/> }
            .into_view()
    }
}

enum SvgImage {
    InMemory(String),
    Request(String),
}

#[component]
fn CacheImage(
    svg: SvgImage,
    #[prop(into)] opt_image: String,
    #[prop(into, optional)] alt: String,
    class: Option<Attribute>,
    priority: bool,
    lazy: bool,
) -> impl IntoView {
    use base64::{engine::general_purpose, Engine as _};

    let style = {
        let background_image = match svg {
            SvgImage::InMemory(svg_data) => {
                let svg_encoded = general_purpose::STANDARD.encode(svg_data.as_bytes());
                format!("url('data:image/svg+xml;base64,{svg_encoded}')")
            }
            SvgImage::Request(svg_url) => {
                format!("url('{}')", svg_url)
            }
        };
        let style= format!(
        "color:transparent;background-size:cover;background-position:50% 50%;background-repeat:no-repeat;background-image:{background_image};",
        );

        style
    };

    let loading = if lazy { "lazy" } else { "eager" };

    view! {
        {if priority {
            view! {  <Link rel="preload" as_="image" href=opt_image.clone()/> }
                .into_view()
        } else {
            view! {  }
                .into_view()
        }}
        <img
            alt=alt.clone()
            class=class
            decoding="async"
            loading=loading
            src=opt_image
            style=style
        />
    }
}
