use serde::{Deserialize, Serialize};

/**
 * Service for creating cached/optimized images!
 */

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash)]
pub struct CachedImage {
    pub src: String,
    pub option: CachedImageOption,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash)]
pub enum CachedImageOption {
    #[serde(rename = "r")]
    Resize(Resize),
    #[serde(rename = "b")]
    Blur(Blur),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash)]
pub struct Resize {
    #[serde(rename = "w")]
    pub width: u32,
    #[serde(rename = "h")]
    pub height: u32,
    #[serde(rename = "q")]
    pub quality: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash)]
pub struct Blur {
    #[serde(rename = "w")]
    pub width: u32,
    #[serde(rename = "h")]
    pub height: u32,
    #[serde(rename = "sw")]
    pub svg_width: u32,
    #[serde(rename = "sh")]
    pub svg_height: u32,
    #[serde(rename = "s")]
    pub sigma: u8,
}

#[cfg(feature = "ssr")]
#[derive(Debug)]
pub enum CreateImageError {
    ImageError(image::ImageError),
    JoinError(tokio::task::JoinError),
    IOError(std::io::Error),
}

impl CachedImage {
    pub(crate) fn get_url_encoded(&self) -> String {
        // TODO: make this configurable?
        let image_cache_path = "/cache/image";
        let params = serde_qs::to_string(&self).unwrap();
        format!("{}?{}", image_cache_path, params)
    }

    #[cfg(feature = "ssr")]
    pub fn get_file_path(&self) -> String {
        use base64::{engine::general_purpose, Engine as _};
        // I'm worried this name will become too long.
        // names are limited to 255 bytes on most filesystems.

        let encode = serde_qs::to_string(&self).unwrap();
        let encode = general_purpose::STANDARD.encode(encode);

        let mut path = path_from_segments(vec!["cache/image", &encode, &self.src]);

        if let CachedImageOption::Resize { .. } = self.option {
            path.set_extension("webp");
        } else {
            path.set_extension("svg");
        };

        path.as_path().to_string_lossy().to_string()
    }

    // TODO: Fix this. Super Yuck.
    #[allow(dead_code)]
    #[cfg(feature = "ssr")]
    pub(crate) fn from_file_path(path: &str) -> Option<Self> {
        use base64::{engine::general_purpose, Engine as _};
        path.split('/')
            .filter_map(|s| {
                general_purpose::STANDARD
                    .decode(s)
                    .ok()
                    .and_then(|s| String::from_utf8(s).ok())
            })
            .find_map(|encoded| serde_qs::from_str(&encoded).ok())
    }

    #[cfg(feature = "ssr")]
    pub(crate) fn get_file_path_from_root(&self, root: &str) -> String {
        let path = path_from_segments(vec![root, &self.get_file_path()]);
        path.as_path().to_string_lossy().to_string()
    }

    #[cfg(feature = "ssr")]
    pub(crate) fn from_url_encoded(url: &str) -> Result<CachedImage, serde_qs::Error> {
        let url = url.split('?').filter(|s| *s != "?").last().unwrap_or(url);
        let result: Result<CachedImage, serde_qs::Error> = serde_qs::from_str(url);
        result
    }

    /// Returns the relative path as a string of the created image (relative from `root`).
    /// Also returns a bool indicating if the image was created (rather than already existing).
    #[cfg(feature = "ssr")]
    pub async fn create_image(&self, root: &str) -> Result<(String, bool), CreateImageError> {
        let option = if let CachedImageOption::Resize(_) = self.option {
            "Resize"
        } else {
            "Blur"
        };
        log::debug!("Creating {option} image for {}", &self.src);
        let relative_path_created = self.get_file_path();

        let save_path = path_from_segments(vec![root, &relative_path_created]);
        let absolute_src_path = path_from_segments(vec![root, &self.src]);

        if file_exists(&save_path).await {
            Ok((relative_path_created, false))
        } else {
            let task = tokio::task::spawn_blocking({
                let config = self.clone();
                move || create_optimized_image(config.option, absolute_src_path, save_path)
            });

            match task.await {
                Err(join_error) => Err(CreateImageError::JoinError(join_error)),
                Ok(Err(err)) => Err(err),
                Ok(Ok(_)) => Ok((relative_path_created, true)),
            }
        }
    }
}

#[cfg(feature = "ssr")]
fn create_optimized_image<P>(
    config: CachedImageOption,
    source_path: P,
    save_path: P,
) -> Result<(), CreateImageError>
where
    P: AsRef<std::path::Path> + AsRef<std::ffi::OsStr>,
{
    use webp::*;

    match config {
        CachedImageOption::Resize(Resize {
            width,
            height,
            quality,
        }) => {
            // it doesn't support svg yet. how can I fix that?
            let path: &std::path::Path = source_path.as_ref();
            let source_path: &std::path::Path = source_path.as_ref();
            let save_path: &std::path::Path = save_path.as_ref();

            // check if the file exists and if it's an svg.

            if path.extension() == Some(std::ffi::OsStr::new("svg")) && path.exists() {
                return convert_svg_to_webp(&source_path, &save_path, width, height, quality);
            } else {
                let img = image::open(&source_path).map_err(|e| CreateImageError::ImageError(e))?;
                let new_img = img.resize(
                    width,
                    height,
                    // Cubic Filter.
                    image::imageops::FilterType::CatmullRom,
                );
                // Create the WebP encoder for the above image
                let encoder: Encoder = Encoder::from_image(&new_img).unwrap();
                // Encode the image at a specified quality 0-100
                let webp: WebPMemory = encoder.encode(quality as f32);
                create_nested_if_needed(&save_path).map_err(|e| CreateImageError::IOError(e))?;
                std::fs::write(save_path, &*webp).map_err(|e| CreateImageError::IOError(e))
            }
        }
        CachedImageOption::Blur(blur) => {
            let svg = create_image_blur(source_path, blur)?;
            create_nested_if_needed(&save_path).map_err(|e| CreateImageError::IOError(e))?;
            std::fs::write(save_path, &*svg).map_err(|e| CreateImageError::IOError(e))
        }
    }
}

#[cfg(feature = "ssr")]
fn create_image_blur<P>(source_path: P, blur: Blur) -> Result<String, CreateImageError>
where
    P: AsRef<std::path::Path> + AsRef<std::ffi::OsStr>,
{
    use webp::*;

    // it doesn't support svg yet. how can I fix that?
    let path: &std::path::Path = source_path.as_ref();

    if path.extension() == Some(std::ffi::OsStr::new("svg")) {
        return make_blur_svg(path, blur);
    }

    let img = image::open(source_path).map_err(|e| CreateImageError::ImageError(e))?;

    let Blur {
        width,
        height,
        svg_height,
        svg_width,
        sigma,
    } = blur;

    let img = img.resize(width, height, image::imageops::FilterType::Nearest);

    // Create the WebP encoder for the above image
    let encoder: Encoder = Encoder::from_image(&img).unwrap();
    // Encode the image at a specified quality 0-100
    let webp: WebPMemory = encoder.encode(80.0);

    // Encode the image to base64
    use base64::{engine::general_purpose, Engine as _};
    let encoded = general_purpose::STANDARD.encode(&*webp);

    let uri = format!("data:image/webp;base64,{}", encoded);

    let svg = format!(
        r#"
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100%" height="100%" viewBox="0 0 {svg_width} {svg_height}" preserveAspectRatio="none">
    <filter id="a" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"> 
        <feGaussianBlur stdDeviation="{sigma}" edgeMode="duplicate"/> 
        <feComponentTransfer>
            <feFuncA type="discrete" tableValues="1 1"/> 
        </feComponentTransfer> 
    </filter> 
    <image filter="url(#a)" x="0" y="0" height="100%" width="100%" href="{uri}"/>
</svg>
"#,
    );

    Ok(svg)
}

#[cfg(feature = "ssr")]
fn path_from_segments(segments: Vec<&str>) -> std::path::PathBuf {
    segments
        .into_iter()
        .map(|s| s.trim_start_matches('/'))
        .map(|s| s.trim_end_matches('/'))
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(feature = "ssr")]
async fn file_exists<P>(path: P) -> bool
where
    P: AsRef<std::path::Path>,
{
    tokio::fs::metadata(path).await.is_ok()
}

#[cfg(feature = "ssr")]
fn create_nested_if_needed<P>(path: P) -> std::io::Result<()>
where
    P: AsRef<std::ffi::OsStr>,
{
    match std::path::Path::new(&path).parent() {
        Some(p) if (!(p).exists()) => std::fs::create_dir_all(p),
        Some(_) => Result::Ok(()),
        None => Result::Ok(()),
    }
}

#[cfg(feature = "ssr")]
fn convert_svg_to_webp<P>(
    source_path: P,
    save_path: P,
    width: u32,
    height: u32,
    quality: u8,
) -> Result<(), CreateImageError>
where
    P: AsRef<std::path::Path> + AsRef<std::ffi::OsStr>,
{
    use std::path::Path;

    use svg_to_png::{
        render::{make_surface_into_dynamic_image, render_image},
        svgs::get_svg_handler,
    };
    use webp::*;

    let path: &std::path::Path = source_path.as_ref();
    let handler = get_svg_handler(Path::new(path));

    let handler = match handler {
        Ok(handler) => handler,
        Err(e) => {
            return Err(CreateImageError::IOError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e,
            )))
        }
    };

    let mut surface =
        // if you want to resize the image then you can use the width and height parameters
        render_image(handler.width, handler.height, handler.handle, None).map_err(|e| {
            CreateImageError::IOError(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;

    let image = make_surface_into_dynamic_image(&mut surface).map_err(|e| {
        CreateImageError::IOError(std::io::Error::new(std::io::ErrorKind::Other, e))
    })?;

    let image = image.resize(width, height, image::imageops::FilterType::Nearest);

    // Create the WebP encoder for the above image
    let encoder: Encoder = Encoder::from_image(&image).map_err(|e| {
        CreateImageError::IOError(std::io::Error::new(std::io::ErrorKind::Other, e))
    })?;

    // Encode the image at a specified quality 0-100
    let webp: WebPMemory = encoder.encode(quality as f32);
    create_nested_if_needed(&save_path).map_err(|e| CreateImageError::IOError(e))?;
    std::fs::write(save_path, &*webp).map_err(|e| CreateImageError::IOError(e))
}

#[cfg(feature = "ssr")]
fn make_blur_svg<P>(source_path: P, blur: Blur) -> Result<String, CreateImageError>
where
    P: AsRef<std::path::Path> + AsRef<std::ffi::OsStr>,
{
    use std::path::Path;

    use svg_to_png::{
        render::{make_surface_into_dynamic_image, render_image},
        svgs::get_svg_handler,
    };
    use webp::*;

    let path: &std::path::Path = source_path.as_ref();
    let handler = get_svg_handler(Path::new(path));

    let handler = match handler {
        Ok(handler) => handler,
        Err(e) => {
            return Err(CreateImageError::IOError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e,
            )))
        }
    };

    let Blur {
        width,
        height,
        svg_height,
        svg_width,
        sigma,
    } = blur;

    let mut surface =
        // if you want to resize the image then you can use the width and height parameters
        render_image(handler.width, handler.height, handler.handle, None).map_err(|e| {
            CreateImageError::IOError(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;

    let image = make_surface_into_dynamic_image(&mut surface).map_err(|e| {
        CreateImageError::IOError(std::io::Error::new(std::io::ErrorKind::Other, e))
    })?;

    let image = image.resize(width, height, image::imageops::FilterType::Nearest);

    // Create the WebP encoder for the above image
    let encoder: Encoder = Encoder::from_image(&image).map_err(|e| {
        CreateImageError::IOError(std::io::Error::new(std::io::ErrorKind::Other, e))
    })?;
    // Encode the image at a specified quality 0-100
    let webp: WebPMemory = encoder.encode(80.0);

    // Encode the image to base64
    use base64::{engine::general_purpose, Engine as _};
    let encoded = general_purpose::STANDARD.encode(&*webp);

    let uri = format!("data:image/webp;base64,{}", encoded);
    // Encode the image at a specified quality 0-100
    let uri = format!("data:image/webp;base64,{}", encoded);

    let svg = format!(
        r#"
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100%" height="100%" viewBox="0 0 {svg_width} {svg_height}" preserveAspectRatio="none">
    <filter id="a" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"> 
        <feGaussianBlur stdDeviation="{sigma}" edgeMode="duplicate"/> 
        <feComponentTransfer>
            <feFuncA type="discrete" tableValues="1 1"/> 
        </feComponentTransfer> 
    </filter> 
    <image filter="url(#a)" x="0" y="0" height="100%" width="100%" href="{uri}"/>
</svg>
"#,
    );

    Ok(svg)
}

// Test module

#[cfg(test)]
mod optimizer_tests {
    use crate::optimizer::*;
    #[test]
    fn url_encode() {
        let img = CachedImage {
            src: "test.jpg".to_string(),
            option: CachedImageOption::Resize(Resize {
                quality: 75,
                width: 100,
                height: 100,
            }),
        };

        let encoded = img.get_url_encoded();
        let decoded: CachedImage = CachedImage::from_url_encoded(&encoded).unwrap();

        dbg!(encoded);
        assert!(img == decoded);
    }

    const TEST_IMAGE: &str = "example/image-example/public/cute_ferris.png";

    #[test]
    fn file_path() {
        let spec = CachedImage {
            src: TEST_IMAGE.to_string(),
            option: CachedImageOption::Blur(Blur {
                width: 25,
                height: 25,
                svg_height: 100,
                svg_width: 100,
                sigma: 20,
            }),
        };

        let file_path = spec.get_file_path();

        dbg!(spec.get_file_path());

        let result = CachedImage::from_file_path(&file_path).unwrap();

        assert_eq!(spec, result);
    }

    #[test]
    fn create_blur() {
        let result = create_image_blur(
            TEST_IMAGE.to_string(),
            Blur {
                width: 25,
                height: 25,
                svg_height: 100,
                svg_width: 100,
                sigma: 20,
            },
        );
        assert!(result.is_ok());
        println!("{}", result.unwrap());
    }

    #[test]
    fn create_blur_svg() {
        let result = create_image_blur(
            "example/image-example-actix/assets/example.svg".to_string(),
            Blur {
                width: 25,
                height: 25,
                svg_height: 100,
                svg_width: 100,
                sigma: 20,
            },
        );
        assert!(result.is_ok());
        println!("{}", result.unwrap());
    }
    #[test]
    fn create_and_save_blur_svg() {
        let spec = CachedImage {
            src: "example/image-example-actix/assets/example.svg".to_string(),
            option: CachedImageOption::Blur(Blur {
                width: 25,
                height: 25,
                svg_height: 100,
                svg_width: 100,
                sigma: 20,
            }),
        };

        let file_path = spec.get_file_path();

        let result = create_optimized_image(spec.option, TEST_IMAGE.to_string(), file_path.clone());

        assert!(result.is_ok());

        println!("Saved SVG at {file_path}");
    }

    #[test]
    fn create_and_save_blur() {
        let spec = CachedImage {
            src: TEST_IMAGE.to_string(),
            option: CachedImageOption::Blur(Blur {
                width: 25,
                height: 25,
                svg_height: 100,
                svg_width: 100,
                sigma: 20,
            }),
        };

        let file_path = spec.get_file_path();

        let result = create_optimized_image(spec.option, TEST_IMAGE.to_string(), file_path.clone());

        assert!(result.is_ok());

        println!("Saved SVG at {file_path}");
    }

    #[test]
    fn create_opt_image() {
        let spec = CachedImage {
            src: TEST_IMAGE.to_string(),
            option: CachedImageOption::Resize(Resize {
                quality: 75,
                width: 100,
                height: 100,
            }),
        };

        let file_path = spec.get_file_path();

        let result = create_optimized_image(spec.option, TEST_IMAGE.to_string(), file_path.clone());

        assert!(result.is_ok());

        println!("Saved WebP at {file_path}");
    }
    #[test]
    fn convert_svg_to_web() {
        let spec = CachedImage {
            src: "example/image-example-actix/assets/example.svg".to_string(),
            option: CachedImageOption::Resize(Resize {
                quality: 75,
                width: 100,
                height: 100,
            }),
        };

        let file_path = spec.get_file_path();

        let result = convert_svg_to_webp(
            "example/image-example-actix/assets/example.svg".to_string(),
            "example/image-example-actix/assets/example.webp".to_string(),
            261,
            271,
            100,
        );

        assert!(result.is_ok());

        println!("Saved WebP at {file_path}");
    }

    #[test]
    fn convert_svg_to_web_with_optimizer() {
        let spec = CachedImage {
            src: "example/image-example-actix/assets/example.svg".to_string(),
            option: CachedImageOption::Resize(Resize {
                quality: 75,
                width: 200,
                height: 200,
            }),
        };

        let file_path = spec.get_file_path();

        let result = create_optimized_image(
            spec.option,
            "example/image-example-actix/assets/example.svg".to_string(),
            file_path.clone(),
        );

        assert!(result.is_ok());

        println!("Saved WebP at {file_path}");
    }
}
