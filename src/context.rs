use ratatui::layout::Rect;
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
};

/// Core shared state for the entire IDE session.
///
/// `SharedContext` provides immutable access to global state like
/// screen dimensions and file context. Uses `Rc<RefCell<>>` for interior mutability
#[derive(Debug, Clone)]
pub struct SharedContext {
    inner: Rc<RefCell<InnerContext>>,
}

#[derive(Debug)]
struct InnerContext {
    screen_area: Rect,
    file_context: FileContext,
}

/// File system state for the current editing session.
#[derive(Debug, Clone)]
pub struct FileContext {
    /// Whether the current file contents have unsaved changes.
    pub file_saved: bool,
    /// Root directory of the project/workspace.
    pub base_path: PathBuf,
    /// Path to the currently active file (relative to base_path), if any.
    pub file_path: Option<PathBuf>,
}

impl SharedContext {
    pub fn new(file_context: FileContext, screen_area: Rect) -> Self {
        let inner = InnerContext::new(file_context, screen_area);
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    /// Returns the current terminal screen dimensions.
    pub fn get_area(&self) -> Rect {
        self.inner.borrow().screen_area
    }

    /// Updates the terminal screen dimensions (called on resize).
    pub fn set_area(&self, screen_area: Rect) {
        self.inner.borrow_mut().screen_area = screen_area;
    }

    /// **Safe read accessor** for file context to avoid double-borrow errors.
    ///
    /// Executes a closure with immutable access to `FileContext`. Designed to prevent
    /// ```
    /// let has_file = context.get_file_context(|fc| -> bool {fc.file_path.is_some()});
    /// let base_dir = context.get_file_context(|fc| -> PathBuf {fc.base_path.clone()});
    /// ```
    pub fn get_file_context<F: FnOnce(&FileContext) -> R, R>(&self, func: F) -> R {
        let file_context = &self.inner.borrow().file_context;
        func(file_context)
    }

    /// **Safe write accessor** for file context to avoid double-borrow errors.
    ///
    /// Executes a closure with mutable access to `FileContext`.
    ///
    /// # Examples
    /// ```
    /// context.set_file_context(|fc| fc.file_saved = true);
    /// context.set_file_context(|fc| fc.file_path = Some(path));
    /// ```
    pub fn set_file_context<F: FnOnce(&mut FileContext)>(&self, func: F) {
        let file_context = &mut self.inner.borrow_mut().file_context;
        func(file_context)
    }
}
impl InnerContext {
    fn new(file_context: FileContext, screen_area: Rect) -> Self {
        Self {
            screen_area,
            file_context,
        }
    }
}
