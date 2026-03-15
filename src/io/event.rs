type Handler = Box<dyn Fn(&str)>;

/// Event (log) manager (C: opj_event_mgr_t).
#[allow(dead_code)]
pub struct EventManager {
    error_handler: Option<Handler>,
    warning_handler: Option<Handler>,
    info_handler: Option<Handler>,
}

impl EventManager {
    pub fn new() -> Self {
        todo!()
    }

    pub fn set_error_handler(&mut self, _handler: impl Fn(&str) + 'static) {
        todo!()
    }

    pub fn set_warning_handler(&mut self, _handler: impl Fn(&str) + 'static) {
        todo!()
    }

    pub fn set_info_handler(&mut self, _handler: impl Fn(&str) + 'static) {
        todo!()
    }

    pub fn error(&self, _msg: &str) {
        todo!()
    }

    pub fn warning(&self, _msg: &str) {
        todo!()
    }

    pub fn info(&self, _msg: &str) {
        todo!()
    }
}

impl Default for EventManager {
    fn default() -> Self {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    #[ignore = "not yet implemented"]
    fn default_no_handlers() {
        let mgr = EventManager::default();
        // Should not panic even without handlers
        mgr.error("test error");
        mgr.warning("test warning");
        mgr.info("test info");
    }

    #[test]
    #[ignore = "not yet implemented"]
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
    #[ignore = "not yet implemented"]
    fn warning_handler_called() {
        let messages = Rc::new(RefCell::new(Vec::new()));
        let msgs = messages.clone();
        let mut mgr = EventManager::new();
        mgr.set_warning_handler(move |msg| msgs.borrow_mut().push(msg.to_string()));
        mgr.warning("watch out");
        assert_eq!(messages.borrow()[0], "watch out");
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn info_handler_called() {
        let messages = Rc::new(RefCell::new(Vec::new()));
        let msgs = messages.clone();
        let mut mgr = EventManager::new();
        mgr.set_info_handler(move |msg| msgs.borrow_mut().push(msg.to_string()));
        mgr.info("progress update");
        assert_eq!(messages.borrow()[0], "progress update");
    }

    #[test]
    #[ignore = "not yet implemented"]
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
        mgr.warning("warn1"); // No handler, should be silent

        assert_eq!(errors.borrow().len(), 1);
        assert_eq!(infos.borrow().len(), 1);
    }
}
