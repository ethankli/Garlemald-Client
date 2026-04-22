//! Regenerate `../assets/icon.icns` and `../assets/icon.ico` from
//! `../assets/icon-source.png`.
//!
//! The source PNG is expected to be a logo on a white (or near-white)
//! background. We:
//!
//!   1. Key out near-white pixels so the rounded-square icon body
//!      becomes the outermost visible shape.
//!   2. Crop to the bounding box of non-transparent pixels.
//!   3. Pad to a square so platform renderers don't stretch the glyph.
//!   4. Emit a multi-resolution `.icns` (macOS) and `.ico` (Windows).
//!
//! Run from the crate root:
//!
//!     cargo run --release
//!
//! or from `garlemald-client/` via `scripts/generate-icons.sh`.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use anyhow::{Context, Result, bail};
use image::{ImageBuffer, Rgba, RgbaImage, imageops::FilterType};

const WHITE_TOLERANCE: u8 = 15;
const ICO_SIZES: &[u32] = &[16, 24, 32, 48, 64, 128, 256];

fn main() -> Result<()> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let project_dir = manifest_dir.parent().context("manifest has no parent")?;
    let assets = project_dir.join("assets");
    let source = assets.join("icon-source.png");
    let out_icns = assets.join("icon.icns");
    let out_ico = assets.join("icon.ico");

    if !source.exists() {
        bail!("source image not found: {}", source.display());
    }

    println!("==> Cleaning and cropping icon-source.png");
    let master = load_and_clean(&source)?;
    println!(
        "    squared master: {}x{}",
        master.width(),
        master.height()
    );

    println!("==> Writing {}", out_icns.display());
    write_icns(&master, &out_icns)?;

    println!("==> Writing {}", out_ico.display());
    write_ico(&master, &out_ico)?;

    println!("done.");
    Ok(())
}

fn load_and_clean(source: &Path) -> Result<RgbaImage> {
    let src = image::open(source).with_context(|| format!("open {}", source.display()))?;
    let mut img = src.to_rgba8();

    let white_min = 255u8.saturating_sub(WHITE_TOLERANCE);
    for px in img.pixels_mut() {
        let [r, g, b, _a] = px.0;
        if r >= white_min && g >= white_min && b >= white_min {
            px.0 = [0, 0, 0, 0];
        }
    }

    let (w, h) = img.dimensions();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0u32, 0u32);
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if img.get_pixel(x, y).0[3] > 0 {
                if !any {
                    min_x = x;
                    max_x = x;
                    min_y = y;
                    max_y = y;
                    any = true;
                } else {
                    if x < min_x {
                        min_x = x;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                    if y < min_y {
                        min_y = y;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        }
    }
    if !any {
        bail!("source image is fully transparent after cleanup");
    }

    let cw = max_x + 1 - min_x;
    let ch = max_y + 1 - min_y;
    let cropped = image::imageops::crop_imm(&img, min_x, min_y, cw, ch).to_image();

    let side = cw.max(ch);
    let mut square: RgbaImage = ImageBuffer::from_pixel(side, side, Rgba([0u8, 0, 0, 0]));
    let ox = ((side - cw) / 2) as i64;
    let oy = ((side - ch) / 2) as i64;
    image::imageops::overlay(&mut square, &cropped, ox, oy);
    Ok(square)
}

fn resize(master: &RgbaImage, size: u32) -> RgbaImage {
    image::imageops::resize(master, size, size, FilterType::Lanczos3)
}

fn write_icns(master: &RgbaImage, out: &Path) -> Result<()> {
    use icns::{IconFamily, IconType, Image as IcnsImage, PixelFormat};

    // (output pixel size, macOS icon OSType). The `_2x` variants are
    // retina pairs: the icon_type records the logical screen size but
    // the encoded image is 2x.
    let spec: &[(u32, IconType)] = &[
        (16, IconType::RGBA32_16x16),
        (32, IconType::RGBA32_16x16_2x),
        (32, IconType::RGBA32_32x32),
        (64, IconType::RGBA32_32x32_2x),
        (128, IconType::RGBA32_128x128),
        (256, IconType::RGBA32_128x128_2x),
        (256, IconType::RGBA32_256x256),
        (512, IconType::RGBA32_256x256_2x),
        (512, IconType::RGBA32_512x512),
        (1024, IconType::RGBA32_512x512_2x),
    ];

    let mut family = IconFamily::new();
    for &(size, icon_type) in spec {
        let resized = resize(master, size);
        let img = IcnsImage::from_data(PixelFormat::RGBA, size, size, resized.into_raw())?;
        family.add_icon_with_type(&img, icon_type)?;
    }

    let file = File::create(out).with_context(|| format!("create {}", out.display()))?;
    family.write(BufWriter::new(file))?;
    Ok(())
}

fn write_ico(master: &RgbaImage, out: &Path) -> Result<()> {
    use ico::{IconDir, IconDirEntry, IconImage, ResourceType};

    let mut dir = IconDir::new(ResourceType::Icon);
    for &size in ICO_SIZES {
        let resized = resize(master, size);
        let img = IconImage::from_rgba_data(size, size, resized.into_raw());
        dir.add_entry(IconDirEntry::encode(&img)?);
    }

    let file = File::create(out).with_context(|| format!("create {}", out.display()))?;
    dir.write(BufWriter::new(file))?;
    Ok(())
}
