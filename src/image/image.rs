use ab_glyph::PxScale;
use derive_builder::Builder;
use image::{GenericImageView, ImageBuffer, Rgb, RgbImage};
use imageproc::drawing::draw_text_mut;
use opencv::core::Size;
use rayon::prelude::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::cell::UnsafeCell;

use crate::util::constants::{
    BLACK_RGB, CASCADIA_FONT, DARK_RGB, GREYSCALE_RAMP, MAGIC_HEIGHT_TO_WIDTH_RATIO, REVERSE_GREYSCALE_RAMP,
    RGB_TO_GREYSCALE, WHITE_RGB,
};
use crate::util::file_util::{check_file_exists, check_valid_file, write_to_file};
use crate::util::{get_size_from_ascii, print_ascii, UnsafeImageBuffer};

#[derive(Builder, Debug)]
#[builder(default)]
pub struct ImageConfig {
    image_path: String,
    scale_down: f32,
    font_size: f32,
    height_sample_scale: f32,
    invert: bool,
    output_file_path: Option<String>,
    output_image_path: Option<String>,
    overwrite: bool,
    custom_chars: Option<Vec<String>>,
}

impl Default for ImageConfig {
    fn default() -> Self {
        ImageConfig {
            image_path: "".to_string(),
            scale_down: 1.0,
            font_size: 12.0,
            height_sample_scale: MAGIC_HEIGHT_TO_WIDTH_RATIO,
            invert: false,
            output_file_path: None,
            output_image_path: None,
            overwrite: false,
            custom_chars: None,
        }
    }
}

#[inline]
pub fn generate_ascii_image(
    ascii: &[Vec<String>],
    size: &Size,
    invert: bool,
    font_size: f32,
) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let background_color = if invert { WHITE_RGB } else { DARK_RGB };
    let text_color = if invert { BLACK_RGB } else { WHITE_RGB };
    //println!("image size: {:?}", size);
    let frame = UnsafeImageBuffer(UnsafeCell::new(Some(RgbImage::from_pixel(
        size.width as u32,
        size.height as u32,
        background_color,
    ))));

    // SAFETY: Operates on pixels independently
    ascii.par_iter().enumerate().for_each(|(row, row_data)| unsafe {
        let text_row = row_data.join("");
        draw_text_mut(
            frame.get().as_mut().unwrap().as_mut().unwrap(),
            text_color,
            0,
            (row as f32 * font_size) as i32,
            PxScale::from(font_size),
            &*CASCADIA_FONT,
            text_row.as_str(),
        );
    });

    frame.0.into_inner().unwrap()
}

pub fn write_to_image<S: AsRef<str>>(
    output_file: S,
    overwrite: bool,
    ascii: &[Vec<String>],
    size: &Size,
    invert: bool,
    font_size: f32,
) {
    let output_file = output_file.as_ref();
    check_file_exists(output_file, overwrite);
    match generate_ascii_image(ascii, size, invert, font_size).save(output_file) {
        Ok(_) => {
            println!("Successfully saved ascii image to {}", output_file);
        }
        Err(e) => {
            eprintln!("Failed to save ascii image to {}: {}", output_file, e);
        }
    }
}

#[inline]
fn convert_image_to_ascii_internal(
    img: image::DynamicImage,
    config: &ImageConfig,
) -> Vec<Vec<String>> {
    let scale_down = config.scale_down;
    let height_sample_scale = config.height_sample_scale;

    // Use custom chars if provided, otherwise use default ramp
    let greyscale_ramp: Vec<String> = if let Some(ref chars) = config.custom_chars {
        chars.clone()
    } else {
        if config.invert {
            REVERSE_GREYSCALE_RAMP.iter().map(|s| s.to_string()).collect()
        } else {
            GREYSCALE_RAMP.iter().map(|s| s.to_string()).collect()
        }
    };

    let (width, height) = img.dimensions();
    let scaled_width = (width as f32 / scale_down) as usize;
    let scaled_height = ((height as f32 / scale_down) / height_sample_scale) as usize;

    let mut res = vec![vec![" ".to_string(); scaled_width]; scaled_height];

    for (y, row) in res.iter_mut().enumerate() {
        for x in 0..scaled_width {
            let pix =
                img.get_pixel((x as f32 * scale_down) as u32, (y as f32 * scale_down * height_sample_scale) as u32);
            if pix[3] > 127 {  // Apply a transparency threshold
                // Standard luminance formula
                let greyscale_value = RGB_TO_GREYSCALE.0 * pix[0] as f32
                    + RGB_TO_GREYSCALE.1 * pix[1] as f32
                    + RGB_TO_GREYSCALE.2 * pix[2] as f32;
                let index = (greyscale_value * (greyscale_ramp.len() - 1) as f32 / 255.0).ceil() as usize;
                row[x] = greyscale_ramp[index].clone();
            }
        }
    }

    res
}

#[inline]
pub fn convert_image_to_ascii(config: &ImageConfig) -> Vec<Vec<String>> {
    let img_path = config.image_path.as_str();
    check_valid_file(img_path);
    let img = image::open(img_path).unwrap_or_else(|_| panic!("Image at {img_path} could not be opened"));
    convert_image_to_ascii_internal(img, config)
}

#[inline]
pub fn convert_image_bytes_to_ascii(
    image_bytes: &[u8],
    config: &ImageConfig,
) -> Vec<Vec<String>> {
    let img = image::load_from_memory(image_bytes)
        .unwrap_or_else(|_| panic!("Could not load image from memory"));
    convert_image_to_ascii_internal(img, config)
}

pub fn process_image(config: ImageConfig) {
    let ascii = convert_image_to_ascii(&config);

    if let Some(file) = config.output_file_path.as_ref() {
        // Check if the output file should be PNG or text based on extension
        if file.to_lowercase().ends_with(".png") {
            write_to_image(
                file,
                config.overwrite,
                &ascii,
                &get_size_from_ascii(&ascii, config.height_sample_scale, config.font_size),
                config.invert,
                config.font_size,
            );
        } else {
            write_to_file(file, config.overwrite, &ascii);
        }
    }

    if let Some(file) = config.output_image_path.as_ref() {
        write_to_image(
            file,
            config.overwrite,
            &ascii,
            &get_size_from_ascii(&ascii, config.height_sample_scale, config.font_size),
            config.invert,
            config.font_size,
        );
    }

    if config.output_file_path.is_none() && config.output_image_path.is_none() {
        print_ascii(&ascii);
    }
}

#[inline]
pub fn write_ascii_to_png(
    ascii: &[Vec<String>],
    output_path: &str,
    height_sample_scale: f32,
    font_size: f32,
    invert: bool,
    overwrite: bool,
) {
    let size = get_size_from_ascii(ascii, height_sample_scale, font_size);
    let image = generate_ascii_image(ascii, &size, invert, font_size);
    
    check_file_exists(output_path, overwrite);
    match image.save(output_path) {
        Ok(_) => {
            println!("Successfully saved ASCII art PNG to {}", output_path);
        }
        Err(e) => {
            eprintln!("Failed to save ASCII art PNG to {}: {}", output_path, e);
        }
    }
}
