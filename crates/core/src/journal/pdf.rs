//! PDF drafts and bounded native rendering. The original PDF remains intact until
//! a checked Journal save. Native handles live only for one background operation.
mod markdown;
mod native;
#[cfg(test)]
mod tests;
pub use markdown::convert;
use native::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
pub const MAX_PDF_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct PdfDocument {
    pub bytes: Arc<[u8]>,
    pub edits: Edits,
    /// Session-only: never written into settings, logs or a sidecar.
    pub password: String,
}
impl PdfDocument {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: bytes.into(),
            edits: Edits::default(),
            password: String::new(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Edits {
    pub fields: BTreeMap<i32, Value>,
    pub annotations: BTreeMap<(usize, usize), Option<String>>,
    pub added: Vec<Annotation>,
    #[serde(default)]
    pub annotation_colors: BTreeMap<(usize, usize), [u8; 3]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Text(String),
    Button(bool),
    Choice {
        selected: Vec<i32>,
        text: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub page: usize,
    pub rect: [f64; 4],
    pub text: String,
    pub note: bool,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}
#[derive(Clone, Debug)]
pub enum FieldKind {
    Text {
        multiline: bool,
        password: bool,
        max_len: usize,
    },
    Button {
        radio: bool,
    },
    Choice {
        items: Vec<String>,
        multiple: bool,
        editable: bool,
    },
    Unsupported,
}
#[derive(Clone, Debug)]
pub struct Field {
    pub id: i32,
    pub name: String,
    pub group: String,
    pub rect: [f64; 4],
    pub kind: FieldKind,
    pub value: Value,
    pub read_only: bool,
}
#[derive(Clone, Debug)]
pub struct ExistingAnnotation {
    pub note: bool,
    pub color: Option<[u8; 3]>,
    pub index: usize,
    pub rect: [f64; 4],
    pub text: String,
    pub read_only: bool,
}
pub struct Page {
    pub number: usize,
    pub count: usize,
    pub size: [f64; 2],
    pub pixels: [usize; 2],
    pub rgba: Vec<u8>,
    pub text: String,
    pub fields: Vec<Field>,
    pub annotations: Vec<ExistingAnnotation>,
    pub can_annotate: bool,
    pub can_fill: bool,
    pub can_copy: bool,
    pub can_add_text: bool,
}
struct Open {
    object: Object,
    count: usize,
    _source: Option<Temporary>,
}
impl Open {
    fn new(pdf: &PdfDocument) -> Result<Self, String> {
        validate_header(&pdf.bytes)?;
        let api = api()?;
        let password = cstring(&pdf.password)?;
        unsafe {
            let mut error = std::ptr::null_mut();
            let mut source = None;
            let doc = if pdf.password.is_empty() {
                let bytes = (api.g_bytes_new)(pdf.bytes.as_ptr(), pdf.bytes.len());
                let doc =
                    (api.poppler_document_new_from_bytes)(bytes, std::ptr::null(), &mut error);
                (api.g_bytes_unref)(bytes);
                doc
            } else {
                // Poppler 26.x's byte-stream password retry can dereference a
                // released stream after a wrong password. Its file loader does
                // not share that path. Keep the private, encrypted source alive
                // until after the document is released (also necessary on Windows).
                let temporary = Temporary::new()?;
                let path = temporary.0.join("source.pdf");
                crate::storage::atomic_write(&path, &pdf.bytes)?;
                let filename = cstring(
                    path.to_str()
                        .ok_or("The temporary PDF path is not UTF-8.")?,
                )?;
                let uri = (api.g_filename_to_uri)(filename.as_ptr(), std::ptr::null(), &mut error);
                if uri.is_null() {
                    return Err(api.error(error));
                }
                let doc = (api.poppler_document_new_from_file)(uri, password.as_ptr(), &mut error);
                (api.g_free)(uri.cast());
                source = Some(temporary);
                doc
            };
            if doc.is_null() {
                return Err(format!("Cannot open PDF: {}", api.error(error)));
            }
            let object = Object(doc, api);
            let count = (api.poppler_document_get_n_pages)(doc);
            if !(1..=100_000).contains(&count) {
                return Err("PDF has no readable pages or exceeds the 100,000 page limit.".into());
            }
            Ok(Self {
                object,
                count: count as usize,
                _source: source,
            })
        }
    }
    fn page(&self, number: usize) -> Result<Object, String> {
        if number >= self.count {
            return Err("This PDF page does not exist.".into());
        }
        let api = self.object.1;
        let ptr = unsafe { (api.poppler_document_get_page)(self.object.0, number as i32) };
        if ptr.is_null() {
            return Err("Could not read this PDF page.".into());
        }
        Ok(Object(ptr, api))
    }
    fn apply(&self, edits: &Edits) -> Result<(), String> {
        let api = self.object.1;
        let doc = self.object.0;
        let permissions = unsafe { (api.poppler_document_get_permissions)(doc) };
        if (!edits.annotations.is_empty()
            || !edits.added.is_empty()
            || !edits.annotation_colors.is_empty())
            && permissions & 8 == 0
        {
            return Err("This PDF does not allow annotations.".into());
        }
        if !edits.fields.is_empty() && permissions & 16 == 0 {
            return Err("This PDF does not allow form filling.".into());
        }
        unsafe {
            for (id, value) in &edits.fields {
                let ptr = (api.poppler_document_get_form_field)(doc, *id);
                if ptr.is_null() {
                    return Err("A PDF form field could not be found. Reload the document.".into());
                }
                let field = Object(ptr, api);
                let ptr = field.0;
                if (api.poppler_form_field_is_read_only)(ptr) != 0 {
                    return Err("This PDF field is read-only.".into());
                }
                match (value, (api.poppler_form_field_get_field_type)(ptr)) {
                    (Value::Text(value), 2) => {
                        let max = (api.poppler_form_field_text_get_max_len)(ptr);
                        if max > 0 && value.chars().count() > max as usize {
                            return Err(format!("This PDF field allows at most {max} characters."));
                        }
                        (api.poppler_form_field_text_set_text)(ptr, cstring(value)?.as_ptr());
                    }
                    (Value::Button(value), 1)
                        if (api.poppler_form_field_button_get_button_type)(ptr) != 0 =>
                    {
                        (api.poppler_form_field_button_set_state)(ptr, i32::from(*value))
                    }
                    (Value::Choice { selected, text }, 3) => {
                        let count = (api.poppler_form_field_choice_get_n_items)(ptr);
                        if selected.iter().any(|i| *i < 0 || *i >= count)
                            || ((api.poppler_form_field_choice_can_select_multiple)(ptr) == 0
                                && selected.len() > 1)
                        {
                            return Err("Invalid PDF choice selection.".into());
                        }
                        (api.poppler_form_field_choice_unselect_all)(ptr);
                        for index in selected {
                            (api.poppler_form_field_choice_select_item)(ptr, *index);
                        }
                        if let Some(text) = text {
                            if (api.poppler_form_field_choice_is_editable)(ptr) == 0 {
                                return Err("This PDF choice does not accept custom text.".into());
                            }
                            (api.poppler_form_field_choice_set_text)(ptr, cstring(text)?.as_ptr());
                        }
                    }
                    _ => return Err("This PDF field type cannot be edited.".into()),
                }
            }
            let mut pages: Vec<_> = edits
                .annotations
                .keys()
                .chain(edits.annotation_colors.keys())
                .map(|(p, _)| *p)
                .collect();
            pages.sort_unstable();
            pages.dedup();
            for number in pages {
                let page = self.page(number)?;
                let list = Mappings::annotations(page.0, api);
                for (index, mapping) in list.iter().enumerate() {
                    let key = (number, index);
                    let value = edits.annotations.get(&key);
                    let color = edits.annotation_colors.get(&key);
                    if value.is_some() || color.is_some() {
                        let typ = (api.poppler_annot_get_annot_type)(mapping.object);
                        if !matches!(typ, 1 | 3)
                            || (api.poppler_annot_get_flags)(mapping.object) & (64 | 128 | 512) != 0
                        {
                            return Err("This PDF annotation is read-only.".into());
                        }
                        if let Some(color) = color
                            && !value.is_some_and(Option::is_none)
                        {
                            if typ != 1 {
                                return Err("Only PDF notes support display colors.".into());
                            }
                            api.set_annotation_color(mapping.object, *color);
                        }
                        if let Some(Some(text)) = value {
                            if typ == 3 {
                                api.hide_text_border(mapping.object);
                            }
                            (api.poppler_annot_set_contents)(
                                mapping.object,
                                cstring(text)?.as_ptr(),
                            );
                        } else if value.is_some_and(Option::is_none) {
                            (api.poppler_page_remove_annot)(page.0, mapping.object);
                        }
                    }
                }
            }
            for annotation in &edits.added {
                if annotation.text.len() > 65536 {
                    return Err("PDF annotation text must be smaller than 64 KiB.".into());
                }
                let page = self.page(annotation.page)?;
                let [w, h] = page_size(&page)?;
                let [x1, y1, x2, y2] = annotation.rect;
                if ![x1, y1, x2, y2].iter().all(|v| v.is_finite())
                    || x1 < 0.
                    || y1 < 0.
                    || x2 > w
                    || y2 > h
                    || x1 >= x2
                    || y1 >= y2
                {
                    return Err("Place the annotation inside the PDF page.".into());
                }
                // Annotation constructors take PDF coordinates (bottom-left origin).
                let rect = Rectangle {
                    x1,
                    y1: h - y2,
                    x2,
                    y2: h - y1,
                };
                if annotation.note {
                    let ptr = (api.poppler_annot_text_new)(doc, &rect);
                    if ptr.is_null() {
                        return Err("Could not create a PDF note.".into());
                    }
                    let annot = Object(ptr, api);
                    if let Some(color) = annotation.color {
                        api.set_annotation_color(annot.0, color);
                    }
                    (api.poppler_annot_set_contents)(annot.0, cstring(&annotation.text)?.as_ptr());
                    (api.poppler_annot_set_flags)(annot.0, 4);
                    (api.poppler_page_add_annot)(page.0, annot.0);
                } else {
                    api.add_text(doc, page.0, &rect, &annotation.text)?;
                }
            }
        }
        Ok(())
    }
}
struct Mappings {
    ptr: *mut List,
    api: &'static Api,
    annotations: bool,
}
impl Mappings {
    unsafe fn annotations(page: Ptr, api: &'static Api) -> Self {
        Self {
            ptr: unsafe { (api.poppler_page_get_annot_mapping)(page) },
            api,
            annotations: true,
        }
    }
    unsafe fn fields(page: Ptr, api: &'static Api) -> Self {
        Self {
            ptr: unsafe { (api.poppler_page_get_form_field_mapping)(page) },
            api,
            annotations: false,
        }
    }
    fn iter(&self) -> impl Iterator<Item = &Mapping> {
        let mut ptr = self.ptr;
        let mut count = 0;
        std::iter::from_fn(move || {
            if ptr.is_null() || count >= 100_000 {
                return None;
            }
            // SAFETY: the engine owns a well-formed list until Mappings is dropped.
            unsafe {
                let item = &*((*ptr).data.cast::<Mapping>());
                ptr = (*ptr).next;
                count += 1;
                Some(item)
            }
        })
    }
}
impl Drop for Mappings {
    fn drop(&mut self) {
        unsafe {
            if self.annotations {
                (self.api.poppler_page_free_annot_mapping)(self.ptr)
            } else {
                (self.api.poppler_page_free_form_field_mapping)(self.ptr)
            }
        }
    }
}
fn page_size(page: &Object) -> Result<[f64; 2], String> {
    let mut w = 0.;
    let mut h = 0.;
    unsafe { (page.1.poppler_page_get_size)(page.0, &mut w, &mut h) };
    if !w.is_finite() || !h.is_finite() || w <= 0. || h <= 0. || w > 100_000. || h > 100_000. {
        return Err("Unsupported PDF page dimensions.".into());
    }
    Ok([w, h])
}
pub fn validate_header(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_PDF_BYTES {
        return Err("PDFs must be no larger than 64 MiB.".into());
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err("This file does not contain a PDF document.".into());
    }
    Ok(())
}
/// Validate before importing. Password-protected PDFs can be imported and
/// unlocked in the viewer, without persisting the password anywhere.
pub fn validate(bytes: &[u8]) -> Result<(), String> {
    let _guard = lock();
    match Open::new(&PdfDocument::new(bytes.to_vec())) {
        Ok(_) => Ok(()),
        Err(error) if error.to_lowercase().contains("encrypted") => Ok(()),
        Err(error) => Err(error),
    }
}
pub fn render(pdf: &PdfDocument, number: usize, scale: f64) -> Result<Page, String> {
    let _guard = lock();
    let open = Open::new(pdf)?;
    let page = open.page(number)?;
    let api = page.1;
    let [_, original_height] = page_size(&page)?;
    // Keep base annotation indices stable when additions/removals change the list.
    let annotations = unsafe {
        Mappings::annotations(page.0, api)
            .iter()
            .enumerate()
            .filter_map(|(index, mapping)| {
                if !matches!((api.poppler_annot_get_annot_type)(mapping.object), 1 | 3) {
                    return None;
                }
                if pdf
                    .edits
                    .annotations
                    .get(&(number, index))
                    .is_some_and(Option::is_none)
                {
                    return None;
                }
                let text = pdf
                    .edits
                    .annotations
                    .get(&(number, index))
                    .and_then(Clone::clone)
                    .unwrap_or_else(|| {
                        api.string((api.poppler_annot_get_contents)(mapping.object))
                    });
                Some(ExistingAnnotation {
                    note: (api.poppler_annot_get_annot_type)(mapping.object) == 1,
                    color: pdf
                        .edits
                        .annotation_colors
                        .get(&(number, index))
                        .copied()
                        .or_else(|| api.annotation_color(mapping.object)),
                    index,
                    rect: top_rect(mapping.area, original_height),
                    text,
                    read_only: (api.poppler_annot_get_flags)(mapping.object) & (64 | 128 | 512)
                        != 0,
                })
            })
            .collect()
    };
    open.apply(&pdf.edits)?;
    let [w, h] = page_size(&page)?;
    if !scale.is_finite() || scale <= 0. {
        return Err("Invalid PDF zoom.".into());
    }
    let scale = scale
        .min(4096. / w)
        .min(4096. / h)
        .min((12_000_000. / (w * h)).sqrt());
    let width = (w * scale).ceil() as usize;
    let height = (h * scale).ceil() as usize;
    unsafe {
        let surface = Surface(
            (api.cairo_image_surface_create)(0, width as i32, height as i32),
            api,
        );
        if (api.cairo_surface_status)(surface.0) != 0 {
            return Err("Could not allocate the PDF page image.".into());
        }
        let context = Context((api.cairo_create)(surface.0), api);
        (api.cairo_set_source_rgb)(context.0, 1., 1., 1.);
        (api.cairo_paint)(context.0);
        (api.cairo_scale)(context.0, scale, scale);
        // Hide free-text outlines in the viewer without changing the source file.
        for mapping in Mappings::annotations(page.0, api).iter() {
            if (api.poppler_annot_get_annot_type)(mapping.object) == 3 {
                api.hide_text_border(mapping.object);
            }
        }
        (api.poppler_page_render)(page.0, context.0);
        if (api.cairo_status)(context.0) != 0 {
            return Err("Could not render the PDF page.".into());
        }
        (api.cairo_surface_flush)(surface.0);
        let data = (api.cairo_image_surface_get_data)(surface.0);
        let stride = (api.cairo_image_surface_get_stride)(surface.0) as usize;
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let pixel = std::slice::from_raw_parts(data.add(y * stride + x * 4), 4);
                let argb = u32::from_ne_bytes(pixel.try_into().unwrap());
                rgba.extend_from_slice(&[(argb >> 16) as u8, (argb >> 8) as u8, argb as u8, 255]);
            }
        }
        let permissions = (api.poppler_document_get_permissions)(open.object.0);
        let can_copy = permissions & 4 != 0;
        let text = if can_copy {
            api.string((api.poppler_page_get_text)(page.0))
        } else {
            String::new()
        };
        // Form mappings are unrotated; widget annotation mappings include the
        // page rotation. Match their raw rectangles to reuse Poppler's transform.
        let mut crop = Rectangle::default();
        (api.poppler_page_get_crop_box)(page.0, &mut crop);
        let mut widget_rects = BTreeMap::new();
        for mapping in Mappings::annotations(page.0, api).iter() {
            if (api.poppler_annot_get_annot_type)(mapping.object) == 20 {
                let mut raw = Rectangle::default();
                (api.poppler_annot_get_rectangle)(mapping.object, &mut raw);
                raw.x1 -= crop.x1;
                raw.x2 -= crop.x1;
                raw.y1 -= crop.y1;
                raw.y2 -= crop.y1;
                widget_rects.insert(rect_key(raw), mapping.area);
            }
        }
        let mut fields = Mappings::fields(page.0, api)
            .iter()
            .map(|mapping| {
                let transformed = Mapping {
                    area: widget_rects
                        .get(&rect_key(mapping.area))
                        .copied()
                        .unwrap_or(mapping.area),
                    object: mapping.object,
                };
                field(api, &transformed, h)
            })
            .collect::<Result<Vec<_>, _>>()?;
        fields.sort_by(|a, b| {
            a.rect[1]
                .total_cmp(&b.rect[1])
                .then_with(|| a.rect[0].total_cmp(&b.rect[0]))
        });
        Ok(Page {
            number,
            count: open.count,
            size: [w, h],
            pixels: [width, height],
            rgba,
            text,
            fields,
            annotations,
            can_annotate: permissions & 8 != 0,
            can_fill: permissions & 16 != 0,
            can_copy,
            can_add_text: api.text_annotations(),
        })
    }
}
fn rect_key(rect: Rectangle) -> [i64; 4] {
    [rect.x1, rect.y1, rect.x2, rect.y2].map(|n| (n * 1_000_000.).round() as i64)
}
fn top_rect(rect: Rectangle, h: f64) -> [f64; 4] {
    [rect.x1, h - rect.y2, rect.x2, h - rect.y1]
}
unsafe fn field(api: &Api, mapping: &Mapping, h: f64) -> Result<Field, String> {
    unsafe {
        let ptr = mapping.object;
        let id = (api.poppler_form_field_get_id)(ptr);
        let mut name = api.string((api.poppler_form_field_get_alternate_ui_name)(ptr));
        if name.is_empty() {
            name = api.string((api.poppler_form_field_get_name)(ptr));
        }
        if name.is_empty() {
            name = format!("Field {id}");
        }
        let (kind, value) = match (api.poppler_form_field_get_field_type)(ptr) {
            2 => (
                FieldKind::Text {
                    multiline: (api.poppler_form_field_text_get_text_type)(ptr) == 1,
                    password: (api.poppler_form_field_text_is_password)(ptr) != 0,
                    max_len: (api.poppler_form_field_text_get_max_len)(ptr).max(0) as usize,
                },
                Value::Text(api.string((api.poppler_form_field_text_get_text)(ptr))),
            ),
            1 if (api.poppler_form_field_button_get_button_type)(ptr) != 0 => (
                FieldKind::Button {
                    radio: (api.poppler_form_field_button_get_button_type)(ptr) == 2,
                },
                Value::Button((api.poppler_form_field_button_get_state)(ptr) != 0),
            ),
            3 => {
                let count = (api.poppler_form_field_choice_get_n_items)(ptr).max(0);
                if count > 10_000 {
                    return Err("This PDF choice field has too many options.".into());
                }
                let items = (0..count)
                    .map(|i| api.string((api.poppler_form_field_choice_get_item)(ptr, i)))
                    .collect();
                let selected = (0..count)
                    .filter(|i| (api.poppler_form_field_choice_is_item_selected)(ptr, *i) != 0)
                    .collect();
                let editable = (api.poppler_form_field_choice_is_editable)(ptr) != 0;
                let text =
                    editable.then(|| api.string((api.poppler_form_field_choice_get_text)(ptr)));
                (
                    FieldKind::Choice {
                        items,
                        multiple: (api.poppler_form_field_choice_can_select_multiple)(ptr) != 0,
                        editable,
                    },
                    Value::Choice { selected, text },
                )
            }
            _ => (FieldKind::Unsupported, Value::Text(String::new())),
        };
        let group = api.string((api.poppler_form_field_get_name)(ptr));
        Ok(Field {
            id,
            name,
            group,
            rect: top_rect(mapping.area, h),
            kind,
            value,
            read_only: (api.poppler_form_field_is_read_only)(ptr) != 0,
        })
    }
}
struct Temporary(std::path::PathBuf);
impl Temporary {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("amigo-pdf-{}", crate::model::id()));
        let builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        builder.create(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
/// Produce a full PDF (forms remain interactive), never a raster replacement.
pub fn save(pdf: &PdfDocument) -> Result<Vec<u8>, String> {
    if pdf.edits == Edits::default() {
        return Ok(pdf.bytes.to_vec());
    }
    let _guard = lock();
    let open = Open::new(pdf)?;
    open.apply(&pdf.edits)?;
    let api = open.object.1;
    let temp = Temporary::new()?;
    let path = temp.0.join("document.pdf");
    let filename = cstring(
        path.to_str()
            .ok_or("The temporary PDF path is not UTF-8.")?,
    )?;
    unsafe {
        let mut error = std::ptr::null_mut();
        let uri = (api.g_filename_to_uri)(filename.as_ptr(), std::ptr::null(), &mut error);
        if uri.is_null() {
            return Err(api.error(error));
        }
        let result = (api.poppler_document_save)(open.object.0, uri, &mut error);
        (api.g_free)(uri.cast());
        if result == 0 {
            return Err(api.error(error));
        }
    }
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > MAX_PDF_BYTES {
        return Err("The edited PDF exceeds the 64 MiB document limit.".into());
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

/// Find the next matching page, wrapping once. Scanned pages have no text layer.
pub fn find(pdf: &PdfDocument, query: &str, start: usize) -> Result<Option<usize>, String> {
    find_with_cancel(
        pdf,
        query,
        start,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
/// Stop a stale whole-document search between pages so it cannot delay saving.
pub fn find_with_cancel(
    pdf: &PdfDocument,
    query: &str,
    start: usize,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Option<usize>, String> {
    use std::sync::atomic::Ordering;
    if cancel.load(Ordering::Relaxed) {
        return Err("PDF search cancelled.".into());
    }

    if query.trim().is_empty() {
        return Ok(None);
    }
    let _guard = lock();
    let open = Open::new(pdf)?;
    open.apply(&pdf.edits)?;
    let api = open.object.1;
    if unsafe { (api.poppler_document_get_permissions)(open.object.0) } & 4 == 0 {
        return Err("This PDF does not allow text extraction.".into());
    }
    let query = query.to_lowercase();
    for offset in 0..open.count {
        if cancel.load(Ordering::Relaxed) {
            return Err("PDF search cancelled.".into());
        }
        let index = (start + offset) % open.count;
        let page = open.page(index)?;
        let text = unsafe { api.string((api.poppler_page_get_text)(page.0)) };
        if text.to_lowercase().contains(&query) {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

/// Page geometry for a virtualized continuous viewer. No page images are kept.
pub fn page_sizes(
    pdf: &PdfDocument,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<[f64; 2]>, String> {
    use std::sync::atomic::Ordering;
    let _guard = lock();
    if cancel.load(Ordering::Relaxed) {
        return Err("PDF loading cancelled.".into());
    }
    let open = Open::new(pdf)?;
    let mut sizes = Vec::with_capacity(open.count);
    for index in 0..open.count {
        if cancel.load(Ordering::Relaxed) {
            return Err("PDF loading cancelled.".into());
        }
        sizes.push(page_size(&open.page(index)?)?);
    }
    Ok(sizes)
}
