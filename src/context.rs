use std::{cell::{Ref, RefCell, RefMut}, path::PathBuf, rc::Rc};

#[derive(Debug, Clone)]
pub struct SharedContext {
    inner: Rc<RefCell<InnerContext>>
}

#[derive(Debug, Clone)]
pub struct InnerContext {
    pub file_context: FileContext,
}

#[derive(Debug, Clone)]
pub struct FileContext {
    pub file_saved: bool,
    pub base_path: PathBuf,
    pub file_path: Option<PathBuf>,
}

impl SharedContext {
    pub fn new(file_context: FileContext) -> Self {
        Self { inner: Rc::new(RefCell::new(InnerContext::new(file_context))) }
    }

    pub fn borrow(&self) -> Ref<'_, InnerContext> {
        self.inner.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, InnerContext> {
        self.inner.borrow_mut()
    }
}
impl InnerContext {
    pub fn new(file_context: FileContext) -> Self {
        Self {
            file_context,
        }
    }
}