use std::sync::{Arc, OnceLock, Weak};

/// Sometimes you need a parent-child relationship which results in code like:
/// ```rust
/// struct Parent {
///     child: Mutex<Option<Arc<Child>>>
/// }
/// impl Parent {
///     fn new() -> Arc<Self> {
///         let self_ = Arc::new(Self {
///             child: Mutex::new(None)
///         };
///         let parent = Arc::downgrade(&self_);
///         *self_.child.lock().await = Some(Child::new(parent));
///         self_
///     }
/// }
/// struct Child {
///     parent: Weak<Parent>
/// }
/// impl Child {
///     fn new(parent: Weak<Parent>) -> Self {
///         Self { parent }
///     }
///     fn upgrade(&self) -> Arc<Parent> {
///         self.parent.upgrade().unwrap()
///     }
/// }
/// ```
/// This class simplifies the above code by allowing us instead to do:
/// ```rust
/// struct Parent {
///     child: Arc<Child>
/// }
/// impl Parent {
///     fn new() -> Arc<Self> {
///         let self_ = Arc::new(Self {
///             child: Child::new()
///         };
///         self_.child.parent.init(self_.clone());
///         self_
///     }
/// }
/// struct Child {
///     parent: LazyWeak<Parent>
/// }
/// impl Child {
///     fn new() -> Self {
///         Self { parent: LazyWeak::new() }
///     }
///     fn upgrade(&self) -> Arc<Parent> {
///         self.parent.upgrade()
///     }
/// }
/// ```
pub struct LazyWeak<Parent>(OnceLock<Weak<Parent>>);

impl<Parent> LazyWeak<Parent> {
    /// Create an empty `LazyWeak`, which must immediately be followed by `weak.init()`.
    pub fn new() -> Self {
        Self(OnceLock::new())
    }

    /// Must be called within the same scope as `new()`.
    pub fn init(&self, parent: Arc<Parent>) {
        assert!(self.0.get().is_none());
        let parent = Arc::downgrade(&parent);
        self.0.set(parent).unwrap();
        assert!(self.0.get().is_some());
    }

    /// Access the `Arc<Parent>` pointer
    pub fn upgrade(&self) -> Arc<Parent> {
        assert!(self.0.get().is_some());
        self.0.get().unwrap().upgrade().unwrap()
    }
}
