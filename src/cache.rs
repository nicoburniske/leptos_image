/// Caches all images in accessible routes of the app on one go.
/// Use parallelism to limit the number of images being processed at once.
/// The provided path will be where the cached images will be stored.
#[cfg(feature = "ssr")]
pub async fn cache_app_images<IV>(
    root: String,
    app_fn: impl Fn() -> IV + 'static,
    parallelism: usize,
    before_mount: impl Fn() + 'static,
    after_mount: impl Fn() + 'static,
) -> Result<(), crate::optimizer::CreateImageError>
where
    IV: leptos::IntoView + 'static,
{
    use crate::optimizer::CreateImageError;

    let images: Vec<crate::optimizer::CachedImage> =
        crate::introspect::find_app_images_with_mount(app_fn, before_mount, after_mount);

    let futures: Vec<_> = images
        .iter()
        .cloned()
        .map(|img| async {
            let root = root.clone();
            tokio::task::spawn(async move { img.create_image(&root).await })
                .await
                .expect("Failed to spawn task")
        })
        .collect();

    use futures::prelude::*;
    let result: Vec<_> = futures::stream::iter(futures)
        .buffer_unordered(parallelism)
        .collect()
        .await;

    let _ = result
        .into_iter()
        .collect::<Result<Vec<_>, CreateImageError>>()?;

    let image_data = images
        .into_iter()
        .filter_map(|img| match img.option {
            crate::optimizer::CachedImageOption::Blur(_) => {
                let path = img.get_file_path_from_root(&root);
                eprintln!("Caching image: {:?}", path);

                Some((img, path))
            }
            _ => None,
        })
        // Read all svg files into memory.
        .filter_map(|(img, path)| std::fs::read_to_string(path).ok().map(|svg| (img, svg)))
        .collect::<Vec<_>>();

    crate::provider::add_image_cache(image_data);
    Ok(())
}
