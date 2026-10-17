//! No owner-token execution. The broker supplies only a fixed private byte-copy.
fn main() -> std::process::ExitCode {
    if !argentum_execution::current_process_is_lpac() {
        eprintln!("PDF worker requires the product's isolated LPAC execution path");
        return std::process::ExitCode::FAILURE;
    }
    #[cfg(windows)]
    match if std::path::Path::new("input.md").is_file() {
        windows::create()
    } else {
        windows::render()
    } {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("PDF render rejected: {reason}");
            std::process::ExitCode::FAILURE
        }
    }
    #[cfg(not(windows))]
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
mod windows {
    mod create_pdf;
    use argentum_artifacts::sha256;
    pub(super) use create_pdf::create;
    use libloading::os::windows::{
        Library, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
    };
    use serde_json::json;
    use std::{
        ffi::{c_char, c_int, c_void},
        io::{Read, Write},
        path::Path,
    };
    type Ptr = *mut c_void;
    type Result<T> = std::result::Result<T, &'static str>;

    fn read(path: &Path, max: usize) -> Result<Vec<u8>> {
        let mut value = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| "fixed input unavailable")?
            .take(max as u64 + 1)
            .read_to_end(&mut value)
            .map_err(|_| "fixed input read failed")?;
        if value.len() > max {
            return Err("fixed input exceeds limit");
        }
        Ok(value)
    }
    fn write(path: &Path, bytes: &[u8]) -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "output already exists or is unavailable")?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| "output write failed")
    }

    pub(super) fn render() -> Result<()> {
        if std::env::args_os().len() != 1 {
            return Err("worker accepts no caller arguments");
        }
        let cwd = std::env::current_dir().map_err(|_| "private directory unavailable")?;
        let dll = cwd.join("pdfium.dll");
        if sha256(&read(&dll, 16 * 1024 * 1024)?) != argentum_execution::PDFIUM_DLL_SHA256 {
            return Err("pinned PDFium identity differs");
        }
        let bytes = read(&cwd.join("input.pdf"), 10 * 1024 * 1024)?;
        if !bytes.starts_with(b"%PDF-") {
            return Err("missing PDF header");
        }
        // SAFETY: only the fixed, hash-checked x64 non-V8/non-XFA PDFium build is
        // loaded. Search is restricted to this owned copy and Windows system DLLs.
        let library = unsafe {
            Library::load_with_flags(
                &dll,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .map_err(|_| "pinned PDFium could not load")?;
        // SAFETY: signatures match the retained chromium/8100 public headers.
        // Symbol handles borrow the live library. All PDF data and bitmap storage
        // outlive their corresponding handles, and all sizes are checked below.
        unsafe {
            macro_rules! symbol {
                ($name:literal,$ty:ty) => {
                    library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|_| "pinned PDFium ABI is unavailable")?
                };
            }
            let init = symbol!("FPDF_InitLibrary", unsafe extern "system" fn());
            let destroy = symbol!("FPDF_DestroyLibrary", unsafe extern "system" fn());
            let load = symbol!(
                "FPDF_LoadMemDocument64",
                unsafe extern "system" fn(*const c_void, usize, *const c_char) -> Ptr
            );
            let close = symbol!("FPDF_CloseDocument", unsafe extern "system" fn(Ptr));
            let security = symbol!(
                "FPDF_GetSecurityHandlerRevision",
                unsafe extern "system" fn(Ptr) -> c_int
            );
            let form = symbol!("FPDF_GetFormType", unsafe extern "system" fn(Ptr) -> c_int);
            let attachments = symbol!(
                "FPDFDoc_GetAttachmentCount",
                unsafe extern "system" fn(Ptr) -> c_int
            );
            let javascript = symbol!(
                "FPDFDoc_GetJavaScriptActionCount",
                unsafe extern "system" fn(Ptr) -> c_int
            );
            let page_count = symbol!("FPDF_GetPageCount", unsafe extern "system" fn(Ptr) -> c_int);
            let load_page = symbol!(
                "FPDF_LoadPage",
                unsafe extern "system" fn(Ptr, c_int) -> Ptr
            );
            let close_page = symbol!("FPDF_ClosePage", unsafe extern "system" fn(Ptr));
            let width = symbol!("FPDF_GetPageWidthF", unsafe extern "system" fn(Ptr) -> f32);
            let height = symbol!("FPDF_GetPageHeightF", unsafe extern "system" fn(Ptr) -> f32);
            let load_text = symbol!("FPDFText_LoadPage", unsafe extern "system" fn(Ptr) -> Ptr);
            let count_chars = symbol!(
                "FPDFText_CountChars",
                unsafe extern "system" fn(Ptr) -> c_int
            );
            let get_text = symbol!(
                "FPDFText_GetText",
                unsafe extern "system" fn(Ptr, c_int, c_int, *mut u16) -> c_int
            );
            let close_text = symbol!("FPDFText_ClosePage", unsafe extern "system" fn(Ptr));
            let create_bitmap = symbol!(
                "FPDFBitmap_CreateEx",
                unsafe extern "system" fn(c_int, c_int, c_int, *mut c_void, c_int) -> Ptr
            );
            let destroy_bitmap = symbol!("FPDFBitmap_Destroy", unsafe extern "system" fn(Ptr));
            let render = symbol!(
                "FPDF_RenderPageBitmap",
                unsafe extern "system" fn(Ptr, Ptr, c_int, c_int, c_int, c_int, c_int, c_int)
            );
            init();
            let document = load(bytes.as_ptr().cast(), bytes.len(), std::ptr::null());
            if document.is_null() {
                return Err("malformed or encrypted PDF");
            }
            if security(document) != -1
                || form(document) != 0
                || attachments(document) != 0
                || javascript(document) != 0
            {
                return Err("encrypted, interactive or attached content is unsupported");
            }
            let count = page_count(document);
            if !(1..=16).contains(&count) {
                return Err("PDF must have 1 to 16 pages");
            }
            let mut pages = Vec::new();
            let mut total_text = 0usize;
            let mut total_png = 0usize;
            for number in 0..count {
                let page = load_page(document, number);
                if page.is_null() {
                    return Err("page cannot be parsed");
                }
                let w = width(page);
                let h = height(page);
                if !w.is_finite()
                    || !h.is_finite()
                    || w <= 0.0
                    || h <= 0.0
                    || w > 5000.0
                    || h > 5000.0
                {
                    return Err("invalid page dimensions");
                }
                let pixel_width = 1000usize;
                let pixel_height = (h / w * pixel_width as f32).ceil() as usize;
                if !(1..=3000).contains(&pixel_height) {
                    return Err("page aspect ratio exceeds the supported tile profile");
                }
                let text_page = load_text(page);
                if text_page.is_null() {
                    return Err("page text extraction failed");
                }
                let characters = count_chars(text_page);
                if !(0..=100_000).contains(&characters) {
                    return Err("page text limit exceeded");
                }
                // PDFium documents a two-code-unit upper bound per returned
                // character plus NUL. Never size this from an unbounded count.
                let mut buffer = vec![0u16; characters as usize * 2 + 1];
                let written = get_text(text_page, 0, characters, buffer.as_mut_ptr());
                if written < 1 || written as usize > buffer.len() {
                    return Err("invalid extracted UTF-16 length");
                }
                let text = String::from_utf16(&buffer[..written as usize - 1])
                    .map_err(|_| "invalid extracted Unicode")?;
                total_text += text.len();
                if total_text > 512 * 1024 {
                    return Err("total extracted text limit exceeded");
                }
                close_text(text_page);
                let mut tiles = Vec::new();
                let mut offset = 0;
                loop {
                    let tile_height = 800usize.min(pixel_height - offset);
                    let mut bgra = vec![255u8; pixel_width * tile_height * 4];
                    // Format 4 is FPDFBitmap_BGRA. The full page is rendered into
                    // a clipped tile with a negative y offset, not stretched.
                    let bitmap = create_bitmap(
                        pixel_width as c_int,
                        tile_height as c_int,
                        4,
                        bgra.as_mut_ptr().cast(),
                        pixel_width as c_int * 4,
                    );
                    if bitmap.is_null() {
                        return Err("bitmap allocation failed");
                    }
                    render(
                        bitmap,
                        page,
                        0,
                        -(offset as c_int),
                        pixel_width as c_int,
                        pixel_height as c_int,
                        0,
                        0,
                    );
                    destroy_bitmap(bitmap);
                    let gray = bgra
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|p| {
                            ((u32::from(p[2]) * 299
                                + u32::from(p[1]) * 587
                                + u32::from(p[0]) * 114)
                                / 1000) as u8
                        })
                        .collect::<Vec<_>>();
                    let mut png = Vec::new();
                    {
                        let mut encoder =
                            png::Encoder::new(&mut png, pixel_width as u32, tile_height as u32);
                        encoder.set_color(png::ColorType::Grayscale);
                        encoder.set_depth(png::BitDepth::Eight);
                        let mut writer =
                            encoder.write_header().map_err(|_| "PNG encoder failed")?;
                        writer
                            .write_image_data(&gray)
                            .map_err(|_| "PNG encoding failed")?;
                    }
                    if png.len() > 128 * 1024 {
                        return Err("rendered tile exceeds the existing model-image byte limit");
                    }
                    total_png += png.len();
                    if total_png > 8 * 1024 * 1024 {
                        return Err("total rendered image limit exceeded");
                    }
                    let name = format!("{}.png", uuid::Uuid::new_v4());
                    write(&cwd.join("output").join(&name), &png)?;
                    tiles.push(json!({"artifact":name,"sha256":sha256(&png),"width":pixel_width,"height":tile_height,"y":offset}));
                    if offset + tile_height == pixel_height {
                        break;
                    }
                    offset += tile_height.saturating_sub(80);
                }
                pages.push(json!({"number":number+1,"width_points":w,"height_points":h,
                    "pixel_height":pixel_height,"text":text,"tiles":tiles}));
                close_page(page);
            }
            close(document);
            destroy();
            let manifest = serde_json::to_vec_pretty(&json!({"schema_version":1,"pdf_sha256":sha256(&bytes),
                "renderer":"PDFium chromium/8100 non-V8 non-XFA","renderer_sha256":argentum_execution::PDFIUM_DLL_SHA256,
                "page_count":count,"pages":pages,"appearance_correctness":"not established by rendering"}))
                .map_err(|_|"render manifest encoding failed")?;
            if manifest.len() > 1024 * 1024 {
                return Err("render manifest exceeds limit");
            }
            write(&cwd.join("output").join("render.json"), &manifest)?;
        }
        Ok(())
    }
}
