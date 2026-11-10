//! What the file viewer asks of macOS: a PDF's pages drawn as pictures
//! (Core Graphics), and the apps that can open a file (Launch Services, through
//! NSWorkspace).

use std::path::{Path, PathBuf};

use objc2_core_foundation::{CGPoint, CGRect, CGSize, CFURL};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGPDFBox, CGPDFDocument, CGPDFPage,
};

/// The first `max` pages of the PDF at `path`, each `width` pixels wide on
/// white, as RGBA, and how many pages it has in all.
pub fn render(
    path: &Path,
    max: usize,
    width: u32,
) -> Result<(Vec<image::RgbaImage>, usize), String> {
    let url = CFURL::from_file_path(path).ok_or("This file isn't there anymore.")?;
    let doc = CGPDFDocument::with_url(Some(&url)).ok_or("This PDF couldn't be read.")?;
    if CGPDFDocument::is_encrypted(Some(&doc)) && !CGPDFDocument::is_unlocked(Some(&doc)) {
        return Err("This PDF is locked with a password.".into());
    }
    let total = CGPDFDocument::number_of_pages(Some(&doc));
    if total == 0 {
        return Err("This PDF has no pages.".into());
    }
    let space = CGColorSpace::new_device_rgb().ok_or("Couldn't draw the PDF.")?;
    let mut pages = Vec::new();
    // Pages are numbered from 1.
    for n in 1..=total.min(max) {
        let Some(page) = CGPDFDocument::page(Some(&doc), n) else {
            continue;
        };
        let rect = CGPDFPage::box_rect(Some(&page), CGPDFBox::CropBox);
        let turned = CGPDFPage::rotation_angle(Some(&page)).rem_euclid(180) == 90;
        let (pw, ph) = if turned {
            (rect.size.height, rect.size.width)
        } else {
            (rect.size.width, rect.size.height)
        };
        if pw <= 0. || ph <= 0. {
            continue;
        }
        let scale = width as f64 / pw;
        let (w, h) = (width as usize, ((ph * scale).round() as usize).max(1));
        let mut buf = vec![255u8; w * h * 4];
        let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
        // SAFETY: `buf` holds `h` rows of `w * 4` bytes and outlives the context,
        // which is dropped at the end of this iteration.
        let ctx = unsafe {
            CGBitmapContextCreate(buf.as_mut_ptr().cast(), w, h, 8, w * 4, Some(&space), info)
        }
        .ok_or("Couldn't draw the PDF.")?;
        CGContext::set_rgb_fill_color(Some(&ctx), 1., 1., 1., 1.);
        CGContext::fill_rect(
            Some(&ctx),
            CGRect::new(CGPoint::new(0., 0.), CGSize::new(w as f64, h as f64)),
        );
        CGContext::scale_ctm(Some(&ctx), scale, scale);
        // Turns the page upright and puts its box at the origin.
        let fit = CGPDFPage::drawing_transform(
            Some(&page),
            CGPDFBox::CropBox,
            CGRect::new(CGPoint::new(0., 0.), CGSize::new(pw, ph)),
            0,
            true,
        );
        CGContext::concat_ctm(Some(&ctx), fit);
        CGContext::draw_pdf_page(Some(&ctx), Some(&page));
        drop(ctx);
        if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, buf) {
            pages.push(img);
        }
    }
    if pages.is_empty() {
        return Err("This PDF couldn't be drawn.".into());
    }
    Ok((pages, total))
}

/// The apps that can open `path`, its default app first, as (name, the
/// app's path). At most a dozen.
pub fn apps_for(path: &Path) -> Vec<(String, PathBuf)> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let ws = NSWorkspace::sharedWorkspace();
    let to_path = |u: &NSURL| u.path().map(|p| PathBuf::from(p.to_string()));
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let mut add = |app: PathBuf| {
        let name = app
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !name.is_empty() && !out.iter().any(|(n, _)| *n == name) && out.len() < 12 {
            out.push((name, app));
        }
    };
    if let Some(p) = ws
        .URLForApplicationToOpenURL(&url)
        .as_deref()
        .and_then(to_path)
    {
        add(p);
    }
    for u in ws.URLsForApplicationsToOpenURL(&url).iter() {
        if let Some(p) = to_path(&u) {
            add(p);
        }
    }
    out
}
