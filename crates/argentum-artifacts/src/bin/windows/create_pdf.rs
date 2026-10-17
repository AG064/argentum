//! Fixed local Markdown creation, using the same pinned non-V8 PDFium DLL.
use super::{read, write, Ptr, Result};
use argentum_artifacts::{layout, markdown, sha256};
use libloading::os::windows::{
    Library, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use std::ffi::{c_int, c_void};

#[repr(C)]
struct FileWrite {
    version: c_int,
    write: unsafe extern "C" fn(*mut FileWrite, *const c_void, u32) -> c_int,
    bytes: Vec<u8>,
    failed: bool,
}

unsafe extern "C" fn write_block(writer: *mut FileWrite, data: *const c_void, size: u32) -> c_int {
    // SAFETY: PDFium synchronously calls this with the live repr(C) writer and
    // a valid buffer of `size` bytes. The vector never grows above 10 MiB.
    let writer = unsafe { &mut *writer };
    if data.is_null() || writer.bytes.len().saturating_add(size as usize) > 10 * 1024 * 1024 {
        writer.failed = true;
        return 0;
    }
    writer
        .bytes
        .extend_from_slice(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) });
    1
}

pub(crate) fn create() -> Result<()> {
    if std::env::args_os().len() != 1 {
        return Err("worker accepts no arguments");
    }
    let cwd = std::env::current_dir().map_err(|_| "private directory unavailable")?;
    let font_bytes = read(&cwd.join("NotoSans.ttf"), 4 * 1024 * 1024)?;
    if sha256(&font_bytes) != argentum_execution::ARTIFACT_FONT_SHA256 {
        return Err("pinned font identity differs");
    }
    let source = read(&cwd.join("input.md"), markdown::MAX_MARKDOWN_BYTES)?;
    let source = std::str::from_utf8(&source).map_err(|_| "Markdown must be UTF-8")?;
    let document = markdown::parse(source)
        .map_err(|_| "unsupported Markdown; use the declared local document profile")?;
    let plan = layout::compose(&document, &font_bytes)
        .map_err(|_| "layout or Unicode is outside the declared local document profile")?;
    let dll = cwd.join("pdfium.dll");
    if sha256(&read(&dll, 16 * 1024 * 1024)?) != argentum_execution::PDFIUM_DLL_SHA256 {
        return Err("pinned PDFium identity differs");
    }
    // SAFETY: hash-pinned private DLL, system-only dependency search. ABI follows
    // retained PDFium chromium/8100 headers. All font/input buffers remain live.
    let library = unsafe {
        Library::load_with_flags(
            &dll,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }
    .map_err(|_| "pinned PDFium could not load")?;
    unsafe {
        macro_rules! symbol {
            ($name:literal,$ty:ty) => {
                library
                    .get::<$ty>(concat!($name, "\0").as_bytes())
                    .map_err(|_| "pinned PDF creation ABI unavailable")?
            };
        }
        let init = symbol!("FPDF_InitLibrary", unsafe extern "system" fn());
        let destroy = symbol!("FPDF_DestroyLibrary", unsafe extern "system" fn());
        let new_document = symbol!("FPDF_CreateNewDocument", unsafe extern "system" fn() -> Ptr);
        let close_document = symbol!("FPDF_CloseDocument", unsafe extern "system" fn(Ptr));
        let load_font = symbol!(
            "FPDFText_LoadFont",
            unsafe extern "system" fn(Ptr, *const u8, u32, c_int, c_int) -> Ptr
        );
        let close_font = symbol!("FPDFFont_Close", unsafe extern "system" fn(Ptr));
        let new_page = symbol!(
            "FPDFPage_New",
            unsafe extern "system" fn(Ptr, c_int, f64, f64) -> Ptr
        );
        let close_page = symbol!("FPDF_ClosePage", unsafe extern "system" fn(Ptr));
        let create_text = symbol!(
            "FPDFPageObj_CreateTextObj",
            unsafe extern "system" fn(Ptr, Ptr, f32) -> Ptr
        );
        let set_text = symbol!(
            "FPDFText_SetText",
            unsafe extern "system" fn(Ptr, *const u16) -> c_int
        );
        let transform = symbol!(
            "FPDFPageObj_Transform",
            unsafe extern "system" fn(Ptr, f64, f64, f64, f64, f64, f64)
        );
        let insert = symbol!("FPDFPage_InsertObject", unsafe extern "system" fn(Ptr, Ptr));
        let rectangle = symbol!(
            "FPDFPageObj_CreateNewRect",
            unsafe extern "system" fn(f32, f32, f32, f32) -> Ptr
        );
        let fill = symbol!(
            "FPDFPageObj_SetFillColor",
            unsafe extern "system" fn(Ptr, u32, u32, u32, u32) -> c_int
        );
        let stroke = symbol!(
            "FPDFPageObj_SetStrokeColor",
            unsafe extern "system" fn(Ptr, u32, u32, u32, u32) -> c_int
        );
        let stroke_width = symbol!(
            "FPDFPageObj_SetStrokeWidth",
            unsafe extern "system" fn(Ptr, f32) -> c_int
        );
        let draw = symbol!(
            "FPDFPath_SetDrawMode",
            unsafe extern "system" fn(Ptr, c_int, c_int) -> c_int
        );
        let generate = symbol!(
            "FPDFPage_GenerateContent",
            unsafe extern "system" fn(Ptr) -> c_int
        );
        let save = symbol!(
            "FPDF_SaveAsCopy",
            unsafe extern "system" fn(Ptr, *mut FileWrite, u32) -> c_int
        );
        init();
        let pdf = new_document();
        if pdf.is_null() {
            return Err("PDF allocation failed");
        }
        // FPDF_FONT_TRUETYPE=2, CID=true. PDFium generates the ToUnicode map.
        let font = load_font(pdf, font_bytes.as_ptr(), font_bytes.len() as u32, 2, 1);
        if font.is_null() {
            return Err("embedded font creation failed");
        }
        for (number, page) in plan.pages.iter().enumerate() {
            let output = new_page(
                pdf,
                number as c_int,
                f64::from(layout::PAGE_WIDTH),
                f64::from(layout::PAGE_HEIGHT),
            );
            if output.is_null() {
                return Err("PDF page allocation failed");
            }
            for r in &page.rectangles {
                let object = rectangle(r.x, r.y, r.width, r.height);
                if object.is_null() {
                    return Err("table rectangle allocation failed");
                }
                let gray = if r.header { 232 } else { 255 };
                if fill(object, gray, gray, gray, 255) == 0
                    || stroke(object, 145, 150, 158, 255) == 0
                    || stroke_width(object, 0.4) == 0
                    || draw(object, 2, 1) == 0
                {
                    return Err("table style failed");
                }
                insert(output, object);
            }
            for line in &page.text {
                let object = create_text(pdf, font, line.size);
                if object.is_null() {
                    return Err("text object allocation failed");
                }
                let text = line.text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                if set_text(object, text.as_ptr()) == 0 || fill(object, 26, 31, 38, 255) == 0 {
                    return Err("text creation failed");
                }
                transform(
                    object,
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                    f64::from(line.x),
                    f64::from(line.y),
                );
                insert(output, object);
            }
            if generate(output) == 0 {
                return Err("PDF page generation failed");
            }
            close_page(output);
        }
        let mut output = FileWrite {
            version: 1,
            write: write_block,
            bytes: Vec::new(),
            failed: false,
        };
        if save(pdf, &mut output, 2 | 8) == 0
            || output.failed
            || !output.bytes.starts_with(b"%PDF-")
        {
            return Err("PDF save failed or exceeded its bound");
        }
        close_font(font);
        close_document(pdf);
        destroy();
        write(&cwd.join("output/result.pdf"), &output.bytes)?;
        let receipt=serde_json::to_vec_pretty(&serde_json::json!({"schema_version":1,"source_sha256":sha256(source.as_bytes()),
            "pdf_sha256":sha256(&output.bytes),"byte_count":output.bytes.len(),"page_count":plan.pages.len(),
            "font_sha256":argentum_execution::ARTIFACT_FONT_SHA256,"renderer_sha256":argentum_execution::PDFIUM_DLL_SHA256,
            "profile":"local-markdown-a4-v1","correctness":"not established by generation"})).map_err(|_|"creation receipt encoding failed")?;
        write(&cwd.join("output/create.json"), &receipt)?;
    }
    Ok(())
}
