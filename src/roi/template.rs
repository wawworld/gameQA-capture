//! `TemplateMatchDetector` — template matching with two backends:
//!   - `opencv` feature: `matchTemplate` (TM_CCOEFF_NORMED) — fast, requires libclang
//!   - fallback (default): pure-Rust SAD with `image` crate — no native libs required
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

/// Cached grayscale template: `(pixels, width, height)`.
#[cfg(not(feature = "opencv"))]
type GrayTemplate = (Vec<u8>, u32, u32);

/// Result of a single template match attempt.
#[derive(Debug, Clone, Copy)]
pub struct MatchResult {
    /// Top-left corner of the best match (monitor-relative pixels).
    pub x: u32,
    pub y: u32,
    /// Match quality score [0.0, 1.0].
    pub score: f32,
}

/// Template-matching detector.
///
/// Loads templates lazily on first use.
/// Stateless: confirm-frame counting is the caller's responsibility.
#[allow(dead_code)]
pub struct TemplateMatchDetector {
    template_paths: Vec<PathBuf>,
    threshold: f32,

    /// opencv backend: BGR Mat per template.
    /// `None` = not yet attempted, `Some(None)` = failed, `Some(Some(m))` = loaded.
    #[cfg(feature = "opencv")]
    templates: Vec<Option<Option<opencv::core::Mat>>>,

    /// Pure-Rust fallback backend: grayscale (luma) pixels per template.
    /// `None` = not yet attempted, `Some(None)` = failed, `Some(Some(_))` = loaded.
    #[cfg(not(feature = "opencv"))]
    templates_gray: Vec<Option<Option<GrayTemplate>>>,
}

impl TemplateMatchDetector {
    pub fn new(template_paths: Vec<PathBuf>, threshold: f32) -> Self {
        #[cfg(feature = "opencv")]
        let templates = template_paths.iter().map(|_| None).collect();
        #[cfg(not(feature = "opencv"))]
        let templates_gray = template_paths.iter().map(|_| None).collect();
        Self {
            template_paths,
            threshold,
            #[cfg(feature = "opencv")]
            templates,
            #[cfg(not(feature = "opencv"))]
            templates_gray,
        }
    }

    /// Match any loaded template against a raw BGRA frame.
    ///
    /// - `search_roi`: optional `(x, y, w, h)` crop applied before matching.
    /// - `downsample`: scale factor in (0, 1] (1.0 = full resolution, 0.4 = 40%).
    ///
    /// Returns the best `MatchResult` across all templates if score ≥ threshold.
    pub fn detect_raw(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        search_roi: Option<(u32, u32, u32, u32)>,
        downsample: f64,
    ) -> Option<MatchResult> {
        #[cfg(feature = "opencv")]
        return self.detect_opencv(bgra, width, height, search_roi, downsample);
        #[cfg(not(feature = "opencv"))]
        return self.detect_sad(bgra, width, height, search_roi, downsample);
    }

    // ── opencv backend ────────────────────────────────────────────────────────

    #[cfg(feature = "opencv")]
    fn detect_opencv(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        search_roi: Option<(u32, u32, u32, u32)>,
        downsample: f64,
    ) -> Option<MatchResult> {
        use opencv::core::{Mat, Point, Rect, Size, min_max_loc, no_array};
        use opencv::imgcodecs::{imread, IMREAD_COLOR};
        use opencv::imgproc::{
            COLOR_BGRA2BGR, INTER_LINEAR, TM_CCOEFF_NORMED, cvt_color, match_template, resize,
        };

        let src_bgra = unsafe {
            Mat::new_rows_cols_with_data(
                height as i32,
                width as i32,
                opencv::core::CV_8UC4,
                bgra.as_ptr() as *mut std::ffi::c_void,
                opencv::core::Mat_AUTO_STEP,
            )
            .ok()?
        };
        let mut src_bgr = Mat::default();
        cvt_color(&src_bgra, &mut src_bgr, COLOR_BGRA2BGR, 0).ok()?;

        let (search_bgr, off_x, off_y) = if let Some((rx, ry, rw, rh)) = search_roi {
            let rect = Rect::new(rx as i32, ry as i32, rw as i32, rh as i32);
            let crop = Mat::roi(&src_bgr, rect).ok()?;
            let mut owned = Mat::default();
            crop.copy_to(&mut owned).ok()?;
            (owned, rx, ry)
        } else {
            (src_bgr, 0u32, 0u32)
        };

        let effective_scale = if downsample > 0.0 && (downsample - 1.0).abs() > 1e-4 {
            downsample
        } else {
            1.0
        };
        let search_img = if (effective_scale - 1.0).abs() > 1e-4 {
            let new_w = ((search_bgr.cols() as f64) * effective_scale) as i32;
            let new_h = ((search_bgr.rows() as f64) * effective_scale) as i32;
            if new_w < 1 || new_h < 1 {
                return None;
            }
            let mut scaled = Mat::default();
            resize(&search_bgr, &mut scaled, Size::new(new_w, new_h), 0.0, 0.0, INTER_LINEAR)
                .ok()?;
            scaled
        } else {
            search_bgr
        };
        let scale_back = 1.0 / effective_scale;

        let mut best_score = -1.0f32;
        let mut best_x = 0u32;
        let mut best_y = 0u32;

        for (i, path) in self.template_paths.iter().enumerate() {
            if self.templates[i].is_none() {
                let loaded =
                    imread(&path.to_string_lossy(), IMREAD_COLOR).ok().filter(|m| !m.empty());
                self.templates[i] = Some(loaded);
            }
            let raw_tmpl = match self.templates[i].as_ref().and_then(|o| o.as_ref()) {
                Some(t) => t,
                None => continue,
            };
            let tmpl = if (effective_scale - 1.0).abs() > 1e-4 {
                let tw = (raw_tmpl.cols() as f64 * effective_scale) as i32;
                let th = (raw_tmpl.rows() as f64 * effective_scale) as i32;
                if tw < 1 || th < 1 {
                    continue;
                }
                let mut st = Mat::default();
                resize(raw_tmpl, &mut st, Size::new(tw, th), 0.0, 0.0, INTER_LINEAR).ok()?;
                st
            } else {
                raw_tmpl.clone()
            };
            if tmpl.cols() >= search_img.cols() || tmpl.rows() >= search_img.rows() {
                continue;
            }
            let mut result = Mat::default();
            if match_template(&search_img, &tmpl, &mut result, TM_CCOEFF_NORMED, &no_array())
                .is_err()
            {
                continue;
            }
            let mut max_val = 0.0f64;
            let mut max_loc = Point::default();
            if min_max_loc(&result, None, Some(&mut max_val), None, Some(&mut max_loc), &no_array())
                .is_err()
            {
                continue;
            }
            let score = max_val as f32;
            if score > best_score {
                best_score = score;
                best_x = off_x + (max_loc.x as f64 * scale_back) as u32;
                best_y = off_y + (max_loc.y as f64 * scale_back) as u32;
            }
        }

        if best_score >= self.threshold {
            Some(MatchResult { x: best_x, y: best_y, score: best_score })
        } else {
            None
        }
    }

    // ── Pure-Rust SAD fallback ─────────────────────────────────────────────────

    #[cfg(not(feature = "opencv"))]
    fn detect_sad(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        search_roi: Option<(u32, u32, u32, u32)>,
        downsample: f64,
    ) -> Option<MatchResult> {
        // Convert full frame BGRA → grayscale.
        let src_gray = bgra_to_luma(bgra, width, height);

        // Crop to search ROI if specified.
        let (search_gray, search_w, search_h, off_x, off_y) =
            if let Some((rx, ry, rw, rh)) = search_roi {
                let cropped = crop_gray(&src_gray, width, rx, ry, rw, rh);
                (cropped, rw, rh, rx, ry)
            } else {
                (src_gray, width, height, 0u32, 0u32)
            };

        // Downsample the search image.
        let effective_scale = if downsample > 0.0 && (downsample - 1.0).abs() > 1e-4 {
            downsample
        } else {
            1.0
        };
        let (search_ds, ds_w, ds_h) = if (effective_scale - 1.0).abs() > 1e-4 {
            let dw = ((search_w as f64) * effective_scale).max(1.0) as u32;
            let dh = ((search_h as f64) * effective_scale).max(1.0) as u32;
            (nn_downscale(&search_gray, search_w, search_h, dw, dh), dw, dh)
        } else {
            (search_gray, search_w, search_h)
        };
        let scale_back = 1.0 / effective_scale;

        let mut best_score = -1.0f32;
        let mut best_x = 0u32;
        let mut best_y = 0u32;

        for (i, path) in self.template_paths.iter().enumerate() {
            // Lazy-load template as grayscale (once; cache failures as Some(None)).
            if self.templates_gray[i].is_none() {
                let loaded = load_png_as_luma(path);
                self.templates_gray[i] = Some(loaded);
            }
            let (tmpl_raw, raw_tw, raw_th) =
                match self.templates_gray[i].as_ref().and_then(|o| o.as_ref()) {
                    Some(t) => t,
                    None => continue,
                };

            // Scale template to match downsampled search image.
            let (tmpl_ds, tmpl_w, tmpl_h) = if (effective_scale - 1.0).abs() > 1e-4 {
                let tw = ((*raw_tw as f64) * effective_scale).max(1.0) as u32;
                let th = ((*raw_th as f64) * effective_scale).max(1.0) as u32;
                (nn_downscale(tmpl_raw, *raw_tw, *raw_th, tw, th), tw, th)
            } else {
                (tmpl_raw.clone(), *raw_tw, *raw_th)
            };

            // Template must fit strictly inside search image.
            if tmpl_w >= ds_w || tmpl_h >= ds_h {
                continue;
            }

            // Sliding-window SAD: score = 1 − (mean_abs_diff / 255).
            let positions_w = ds_w - tmpl_w;
            let positions_h = ds_h - tmpl_h;
            let tmpl_pixels = (tmpl_w * tmpl_h) as u64;

            let mut local_best_score = -1.0f32;
            let mut local_best_x = 0u32;
            let mut local_best_y = 0u32;

            for py in 0..positions_h {
                for px in 0..positions_w {
                    let sad = sad_window(
                        &search_ds, ds_w, px, py,
                        &tmpl_ds, tmpl_w, tmpl_h,
                    );
                    let score = 1.0 - (sad as f32 / (tmpl_pixels as f32 * 255.0));
                    if score > local_best_score {
                        local_best_score = score;
                        local_best_x = px;
                        local_best_y = py;
                    }
                }
            }

            if local_best_score > best_score {
                best_score = local_best_score;
                best_x = off_x + (local_best_x as f64 * scale_back) as u32;
                best_y = off_y + (local_best_y as f64 * scale_back) as u32;
            }
        }

        if best_score >= self.threshold {
            Some(MatchResult { x: best_x, y: best_y, score: best_score })
        } else {
            None
        }
    }
}

// ── Pure-Rust helpers ─────────────────────────────────────────────────────────

/// Convert a BGRA byte slice to a grayscale (luma) byte slice (Rec.601).
#[cfg(not(feature = "opencv"))]
fn bgra_to_luma(bgra: &[u8], width: u32, height: u32) -> Vec<u8> {
    let n = (width * height) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let b = bgra[i * 4] as u32;
        let g = bgra[i * 4 + 1] as u32;
        let r = bgra[i * 4 + 2] as u32;
        // Rec.601: 0.114*B + 0.587*G + 0.299*R  (scaled by 1000)
        out.push(((114 * b + 587 * g + 299 * r) / 1000) as u8);
    }
    out
}

/// Crop a grayscale image to a sub-region.
#[cfg(not(feature = "opencv"))]
fn crop_gray(src: &[u8], src_w: u32, rx: u32, ry: u32, rw: u32, rh: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((rw * rh) as usize);
    for y in ry..ry + rh {
        let row_start = (y * src_w + rx) as usize;
        out.extend_from_slice(&src[row_start..row_start + rw as usize]);
    }
    out
}

/// Nearest-neighbour downscale of a grayscale image.
#[cfg(not(feature = "opencv"))]
fn nn_downscale(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dst_w * dst_h) as usize];
    for dy in 0..dst_h {
        let sy = ((dy as f64 / dst_h as f64) * src_h as f64) as u32;
        for dx in 0..dst_w {
            let sx = ((dx as f64 / dst_w as f64) * src_w as f64) as u32;
            out[(dy * dst_w + dx) as usize] = src[(sy * src_w + sx) as usize];
        }
    }
    out
}

/// Sum of absolute differences for a single window position.
#[cfg(not(feature = "opencv"))]
#[inline]
fn sad_window(
    search: &[u8],
    search_w: u32,
    px: u32,
    py: u32,
    tmpl: &[u8],
    tmpl_w: u32,
    tmpl_h: u32,
) -> u64 {
    let mut sad: u64 = 0;
    for ty in 0..tmpl_h {
        let s_row = ((py + ty) * search_w + px) as usize;
        let t_row = (ty * tmpl_w) as usize;
        for tx in 0..tmpl_w as usize {
            let diff = (search[s_row + tx] as i32 - tmpl[t_row + tx] as i32).unsigned_abs();
            sad += diff as u64;
        }
    }
    sad
}

/// Load a PNG file and convert to grayscale luma bytes.
/// Returns `None` if the file is missing, unreadable, or not a valid PNG.
#[cfg(not(feature = "opencv"))]
fn load_png_as_luma(path: &PathBuf) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::open(path).ok()?.into_luma8();
    let w = img.width();
    let h = img.height();
    Some((img.into_raw(), w, h))
}
