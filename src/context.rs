use ratatui::layout::Rect;
use std::{
    cell::{Ref, RefCell, RefMut},
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

    pub fn borrow(&self) -> Ref<'_, InnerContext> {
        self.inner.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, InnerContext> {
        self.inner.borrow_mut()
    }

    pub fn set_file_path(&self, path: Option<PathBuf>)  {
        self.borrow_mut().file_context.file_path = path;
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
