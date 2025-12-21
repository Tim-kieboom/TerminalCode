use ratatui::layout::Rect;
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
};


#[derive(Debug, Clone)]
pub struct SharedContext {
    inner: Rc<RefCell<InnerContext>>,
}

#[derive(Debug)]
pub struct InnerContext {
    pub screen_area: Rect,
    pub file_context: FileContext,
}

#[derive(Debug, Clone)]
pub struct FileContext {
    pub file_saved: bool,
    pub base_path: PathBuf,
    pub file_path: Option<PathBuf>,
}

impl SharedContext {
    pub fn new(file_context: FileContext, screen_area: Rect) -> Self {
        let inner = InnerContext::new(file_context, screen_area);
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn get_area(&self) -> Rect {
        self.inner.borrow().screen_area
    }

    pub fn set_area(&self, screen_area: Rect) {
        self.inner.borrow_mut().screen_area = screen_area;
    }

    pub fn get_file_context<F: FnOnce(&FileContext) -> R, R>(&self, func: F) -> R {
        let file_context = &self.inner.borrow().file_context;
        func(file_context)
    }

    pub fn set_file_context<F: FnOnce(&mut FileContext)>(&self, func: F) {
        let file_context = &mut self.inner.borrow_mut().file_context;
        func(file_context)
    }
}
impl InnerContext {
    pub fn new(file_context: FileContext, screen_area: Rect) -> Self {
        Self {
            screen_area,
            file_context,
        }
    }
}
