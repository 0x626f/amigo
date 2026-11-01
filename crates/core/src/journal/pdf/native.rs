//! The stable Poppler GLib / Cairo C ABI, loaded at runtime. No PDF parser or
//! native object crosses threads. All handles are owned by the calling worker.
use libloading::Library;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    sync::{Mutex, OnceLock},
};
pub(super) type Ptr = *mut c_void;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Rectangle {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}
#[repr(C)]
pub(super) struct List {
    pub data: Ptr,
    pub next: *mut List,
    pub prev: *mut List,
}
#[repr(C)]
pub(super) struct Mapping {
    pub area: Rectangle,
    pub object: Ptr,
}
#[repr(C)]
pub(super) struct Error {
    domain: u32,
    pub code: c_int,
    pub message: *mut c_char,
}
#[repr(C)]
pub(super) struct Color {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
}
#[repr(C)]
pub(super) struct Font {
    pub name: *mut c_char,
    pub size: f64,
    pub stretch: c_int,
    pub weight: c_int,
    pub style: c_int,
}
pub(super) type WriteFn = unsafe extern "C" fn(Ptr, *const u8, u32) -> c_int;

macro_rules! api {
    ($($name:ident ($($arg:ty),*) -> $ret:ty;)*) => {
        #[allow(non_snake_case)]
        pub(super) struct Api { $(pub $name: unsafe extern "C" fn($($arg),*) -> $ret,)* _libraries:Vec<Library> }
        impl Api {
            fn load()->Result<Self,String> {
                let libraries = vec![
                    library(&["libpoppler-glib.so.8", "libpoppler-glib.8.dylib", "libpoppler-glib-8.dll"] )?,
                    library(&["libgobject-2.0.so.0", "libgobject-2.0.0.dylib", "libgobject-2.0-0.dll"] )?,
                    library(&["libglib-2.0.so.0", "libglib-2.0.0.dylib", "libglib-2.0-0.dll"] )?,
                    library(&["libcairo.so.2", "libcairo.2.dylib", "libcairo-2.dll"] )?,
                    library(&["libpangocairo-1.0.so.0", "libpangocairo-1.0.0.dylib", "libpangocairo-1.0-0.dll"] )?,
                    library(&["libpango-1.0.so.0", "libpango-1.0.0.dylib", "libpango-1.0-0.dll"] )?,
                ];
                // SAFETY: each declaration below matches the public C ABI. Library
                // ownership outlives every copied function pointer.
                unsafe { Ok(Self { $($name: libraries.iter().find_map(|lib| lib.get(concat!(stringify!($name),"\0").as_bytes()).ok().map(|s| *s)).ok_or_else(||format!("PDF runtime is missing {}. Update Poppler / Cairo / Pango.", stringify!($name)))?,)* _libraries:libraries }) }
            }
        }
    }
}
api! {
    g_bytes_new(*const u8,usize)->Ptr;
    g_bytes_unref(Ptr)->();
    g_object_unref(Ptr)->();
    g_free(Ptr)->();
    g_error_free(*mut Error)->();
    g_filename_to_uri(*const c_char,*const c_char,*mut *mut Error)->*mut c_char;
    poppler_document_new_from_bytes(Ptr,*const c_char,*mut *mut Error)->Ptr;
    poppler_document_new_from_file(*const c_char,*const c_char,*mut *mut Error)->Ptr;
    poppler_document_get_n_pages(Ptr)->c_int;
    poppler_document_get_permissions(Ptr)->c_int;
    poppler_document_get_page(Ptr,c_int)->Ptr;
    poppler_document_get_form_field(Ptr,c_int)->Ptr;
    poppler_document_save(Ptr,*const c_char,*mut *mut Error)->c_int;
    poppler_page_get_size(Ptr,*mut f64,*mut f64)->();
    poppler_page_get_crop_box(Ptr,*mut Rectangle)->();
    poppler_page_render(Ptr,Ptr)->();
    poppler_page_get_text(Ptr)->*mut c_char;
    poppler_page_get_form_field_mapping(Ptr)->*mut List;
    poppler_page_free_form_field_mapping(*mut List)->();
    poppler_page_get_annot_mapping(Ptr)->*mut List;
    poppler_page_free_annot_mapping(*mut List)->();
    poppler_page_add_annot(Ptr,Ptr)->();
    poppler_page_remove_annot(Ptr,Ptr)->();
    poppler_annot_get_annot_type(Ptr)->c_int;
    poppler_annot_get_contents(Ptr)->*mut c_char;
    poppler_annot_get_color(Ptr)->*mut Color;
    poppler_annot_set_color(Ptr,*const Color)->();
    poppler_annot_get_flags(Ptr)->c_int;
    poppler_annot_get_rectangle(Ptr,*mut Rectangle)->();
    poppler_annot_set_contents(Ptr,*const c_char)->();
    poppler_annot_set_flags(Ptr,c_int)->();
    poppler_annot_text_new(Ptr,*const Rectangle)->Ptr;
    poppler_form_field_get_id(Ptr)->c_int;
    poppler_form_field_get_name(Ptr)->*mut c_char;
    poppler_form_field_get_alternate_ui_name(Ptr)->*mut c_char;
    poppler_form_field_get_field_type(Ptr)->c_int;
    poppler_form_field_is_read_only(Ptr)->c_int;
    poppler_form_field_text_get_text(Ptr)->*mut c_char;
    poppler_form_field_text_set_text(Ptr,*const c_char)->();
    poppler_form_field_text_get_max_len(Ptr)->c_int;
    poppler_form_field_text_get_text_type(Ptr)->c_int;
    poppler_form_field_text_is_password(Ptr)->c_int;
    poppler_form_field_button_get_button_type(Ptr)->c_int;
    poppler_form_field_button_get_state(Ptr)->c_int;
    poppler_form_field_button_set_state(Ptr,c_int)->();
    poppler_form_field_choice_get_n_items(Ptr)->c_int;
    poppler_form_field_choice_get_item(Ptr,c_int)->*mut c_char;
    poppler_form_field_choice_is_item_selected(Ptr,c_int)->c_int;
    poppler_form_field_choice_can_select_multiple(Ptr)->c_int;
    poppler_form_field_choice_is_editable(Ptr)->c_int;
    poppler_form_field_choice_get_text(Ptr)->*mut c_char;
    poppler_form_field_choice_set_text(Ptr,*const c_char)->();
    poppler_form_field_choice_unselect_all(Ptr)->();
    poppler_form_field_choice_select_item(Ptr,c_int)->();
    cairo_image_surface_create(c_int,c_int,c_int)->Ptr;
    cairo_image_surface_get_data(Ptr)->*mut u8;
    cairo_image_surface_get_stride(Ptr)->c_int;
    cairo_surface_flush(Ptr)->();
    cairo_surface_mark_dirty(Ptr)->();
    cairo_surface_finish(Ptr)->();
    cairo_surface_status(Ptr)->c_int;
    cairo_surface_destroy(Ptr)->();
    cairo_create(Ptr)->Ptr;
    cairo_status(Ptr)->c_int;
    cairo_destroy(Ptr)->();
    cairo_scale(Ptr,f64,f64)->();
    cairo_translate(Ptr,f64,f64)->();
    cairo_save(Ptr)->();
    cairo_restore(Ptr)->();
    cairo_set_source_rgb(Ptr,f64,f64,f64)->();
    cairo_set_source_surface(Ptr,Ptr,f64,f64)->();
    cairo_paint(Ptr)->();
    cairo_rectangle(Ptr,f64,f64,f64,f64)->();
    cairo_fill(Ptr)->();
    cairo_move_to(Ptr,f64,f64)->();
    cairo_show_page(Ptr)->();
    cairo_pdf_surface_create_for_stream(WriteFn,Ptr,f64,f64)->Ptr;
    cairo_pdf_surface_set_metadata(Ptr,c_int,*const c_char)->();
    pango_cairo_create_layout(Ptr)->Ptr;
    pango_cairo_show_layout_line(Ptr,Ptr)->();
    pango_layout_set_markup(Ptr,*const c_char,c_int)->();
    pango_layout_set_width(Ptr,c_int)->();
    pango_layout_set_wrap(Ptr,c_int)->();
    pango_layout_get_line_count(Ptr)->c_int;
    pango_layout_get_line_readonly(Ptr,c_int)->Ptr;
    pango_layout_line_get_extents(Ptr,Ptr,*mut PangoRectangle)->();
}
#[repr(C)]
#[derive(Default)]
pub(super) struct PangoRectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
fn library(names: &[&str]) -> Result<Library, String> {
    let mut candidates = Vec::new();
    for name in names {
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            candidates.push(parent.join("lib").join(name));
        }
        candidates.push(std::path::PathBuf::from(name));
        if cfg!(target_os = "macos") {
            candidates.push(std::path::Path::new("/opt/homebrew/lib").join(name));
            candidates.push(std::path::Path::new("/usr/local/lib").join(name));
        }
    }
    for path in candidates {
        // SAFETY: load only known system ABI names / the packaged lib directory.
        if let Ok(lib) = unsafe { Library::new(path) } {
            return Ok(lib);
        }
    }
    Err("PDF support needs Poppler GLib, Cairo and Pango. Install poppler-glib (Fedora), libpoppler-glib8 (Debian/Ubuntu), or poppler via Homebrew (macOS). On Windows, place the Poppler GLib runtime and its dependencies in Amigo’s lib folder.".into())
}
static API: OnceLock<Api> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());
pub(super) fn api() -> Result<&'static Api, String> {
    if let Some(api) = API.get() {
        return Ok(api);
    }
    // Failed loads are retryable after installing a missing runtime.
    let api = Api::load()?;
    let _ = API.set(api);
    Ok(API.get().expect("PDF runtime was initialized"))
}
pub(super) fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}
pub(super) fn cstring(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| "PDF text cannot contain a NUL character.".into())
}
impl Api {
    /// Takes ownership of an allocated GLib string.
    pub unsafe fn string(&self, ptr: *mut c_char) -> String {
        if ptr.is_null() {
            return String::new();
        }
        // SAFETY: the caller supplies a transfer-full GLib UTF-8 string.
        unsafe {
            let out = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            (self.g_free)(ptr.cast());
            out
        }
    }
    pub unsafe fn error(&self, ptr: *mut Error) -> String {
        if ptr.is_null() {
            return "The PDF engine could not complete this operation.".into();
        }
        unsafe {
            let text = CStr::from_ptr((*ptr).message)
                .to_string_lossy()
                .into_owned();
            (self.g_error_free)(ptr);
            text
        }
    }
    pub unsafe fn annotation_color(&self, annot: Ptr) -> Option<[u8; 3]> {
        unsafe {
            let color = (self.poppler_annot_get_color)(annot);
            if color.is_null() {
                return None;
            }
            let rgb = [
                (((*color).red as u32 + 128) / 257) as u8,
                (((*color).green as u32 + 128) / 257) as u8,
                (((*color).blue as u32 + 128) / 257) as u8,
            ];
            (self.g_free)(color.cast());
            Some(rgb)
        }
    }
    pub unsafe fn set_annotation_color(&self, annot: Ptr, rgb: [u8; 3]) {
        let color = Color {
            red: rgb[0] as u16 * 257,
            green: rgb[1] as u16 * 257,
            blue: rgb[2] as u16 * 257,
        };
        unsafe {
            (self.poppler_annot_set_color)(annot, &color);
        }
    }
    pub fn text_annotations(&self) -> bool {
        // Probe the complete optional capability, rather than guessing by version.
        [
            b"poppler_annot_free_text_new\0".as_slice(),
            b"poppler_annot_free_text_set_font_desc\0".as_slice(),
            b"poppler_annot_free_text_set_font_color\0".as_slice(),
            b"poppler_annot_set_border_width\0".as_slice(),
        ]
        .iter()
        .all(|symbol| unsafe {
            self._libraries[0]
                .get::<unsafe extern "C" fn()>(symbol)
                .is_ok()
        })
    }
    pub unsafe fn hide_text_border(&self, annotation: Ptr) {
        type SetBorder = unsafe extern "C" fn(Ptr, f64);
        unsafe {
            if let Ok(set) =
                self._libraries[0].get::<SetBorder>(b"poppler_annot_set_border_width\0")
            {
                set(annotation, 0.);
            }
        }
    }
    pub unsafe fn add_text(
        &self,
        document: Ptr,
        page: Ptr,
        rect: &Rectangle,
        text: &str,
    ) -> Result<(), String> {
        type New = unsafe extern "C" fn(Ptr, *const Rectangle) -> Ptr;
        type SetFont = unsafe extern "C" fn(Ptr, *const Font);
        type SetColor = unsafe extern "C" fn(Ptr, *const Color);
        // Free-text creation/font APIs are optional; reading/forms work on older releases.
        unsafe {
            let lib = &self._libraries[0];
            let new = lib
                .get::<New>(b"poppler_annot_free_text_new\0")
                .map_err(|_| "Adding text needs a newer Poppler runtime.".to_string())?;
            let set_font = lib
                .get::<SetFont>(b"poppler_annot_free_text_set_font_desc\0")
                .map_err(|e| e.to_string())?;
            let set_color = lib
                .get::<SetColor>(b"poppler_annot_free_text_set_font_color\0")
                .map_err(|e| e.to_string())?;
            let text = cstring(text)?;
            let annotation = new(document, rect);
            if annotation.is_null() {
                return Err("Could not create the PDF text annotation.".into());
            }
            let font = Font {
                name: c"Sans".as_ptr().cast_mut(),
                size: 12.,
                stretch: 4,
                weight: 400,
                style: 0,
            };
            set_font(annotation, &font);
            set_color(
                annotation,
                &Color {
                    red: 0,
                    green: 0,
                    blue: 0,
                },
            );
            self.hide_text_border(annotation);
            (self.poppler_annot_set_contents)(annotation, text.as_ptr());
            (self.poppler_annot_set_flags)(annotation, 4);
            (self.poppler_page_add_annot)(page, annotation);
            (self.g_object_unref)(annotation);
        }
        Ok(())
    }
}
/// RAII handles cannot escape the worker and keep their runtime loaded forever.
pub(super) struct Object(pub Ptr, pub &'static Api);
impl Drop for Object {
    fn drop(&mut self) {
        unsafe { (self.1.g_object_unref)(self.0) }
    }
}
pub(super) struct Surface(pub Ptr, pub &'static Api);
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { (self.1.cairo_surface_destroy)(self.0) }
    }
}
pub(super) struct Context(pub Ptr, pub &'static Api);
impl Drop for Context {
    fn drop(&mut self) {
        unsafe { (self.1.cairo_destroy)(self.0) }
    }
}
