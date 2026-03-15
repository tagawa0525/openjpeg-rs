type Handler = Box<dyn Fn(&str)>;

/// Event (log) manager (C: opj_event_mgr_t).
pub struct EventManager {
    error_handler: Option<Handler>,
    warning_handler: Option<Handler>,
    info_handler: Option<Handler>,
}

impl EventManager {
    /// Create a new event manager with no handlers.
    pub fn new() -> Self {
        Self {
            error_handler: None,
            warning_handler: None,
            info_handler: None,
        }
    }

    pub fn set_error_handler(&mut self, handler: impl Fn(&str) + 'static) {
        self.error_handler = Some(Box::new(handler));
    }

    pub fn set_warning_handler(&mut self, handler: impl Fn(&str) + 'static) {
        self.warning_handler = Some(Box::new(handler));
    }

    pub fn set_info_handler(&mut self, handler: impl Fn(&str) + 'static) {
        self.info_handler = Some(Box::new(handler));
    }

    /// Send error message to handler (no-op if no handler set).
    pub fn error(&self, msg: &str) {
        if let Some(h) = &self.error_handler {
            h(msg);
        }
    }

    /// Send warning message to handler (no-op if no handler set).
    pub fn warning(&self, msg: &str) {
        if let Some(h) = &self.warning_handler {
            h(msg);
        }
    }

    /// Send info message to handler (no-op if no handler set).
    pub fn info(&self, msg: &str) {
        if let Some(h) = &self.info_handler {
            h(msg);
        }
    }
}

impl Default for EventManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn default_no_handlers() {
        let mgr = EventManager::default();
        mgr.error("test error");
        mgr.warning("test warning");
        mgr.info("test info");
    }

    #[test]
    fn error_handler_called() {
        let messages = Rc::new(RefCell::new(Vec::new()));
        let msgs = messages.clone();
        let mut mgr = EventManager::new();
        mgr.set_error_handler(move |msg| msgs.borrow_mut().push(msg.to_string()));
        mgr.error("something broke");
        assert_eq!(messages.borrow().len(), 1);
        assert_eq!(messages.borrow()[0], "something broke");
    }

    #[test]
    fn warning_handler_called() {
        let messages = Rc::new(RefCell::new(Vec::new()));
        let msgs = messages.clone();
        let mut mgr = EventManager::new();
        mgr.set_warning_handler(move |msg| msgs.borrow_mut().push(msg.to_string()));
        mgr.warning("watch out");
        assert_eq!(messages.borrow()[0], "watch out");
    }

    #[test]
    fn info_handler_called() {
        let messages = Rc::new(RefCell::new(Vec::new()));
        let msgs = messages.clone();
        let mut mgr = EventManager::new();
        mgr.set_info_handler(move |msg| msgs.borrow_mut().push(msg.to_string()));
        mgr.info("progress update");
        assert_eq!(messages.borrow()[0], "progress update");
    }

    #[test]
    fn handlers_independent() {
        let errors = Rc::new(RefCell::new(Vec::new()));
        let infos = Rc::new(RefCell::new(Vec::new()));
        let errs = errors.clone();
        let infs = infos.clone();

        let mut mgr = EventManager::new();
        mgr.set_error_handler(move |msg| errs.borrow_mut().push(msg.to_string()));
        mgr.set_info_handler(move |msg| infs.borrow_mut().push(msg.to_string()));

        mgr.error("err1");
        mgr.info("info1");
        mgr.warning("warn1");

        assert_eq!(errors.borrow().len(), 1);
        assert_eq!(infos.borrow().len(), 1);
    }
}
