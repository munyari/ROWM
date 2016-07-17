extern crate rustwlc;
use rustwlc::types::*;
use rustwlc::callback;
use rustwlc::WlcView;

// Callback must be labelled extern as they will be called from C
extern "C" fn view_created(view: WlcView) -> bool {
    view.bring_to_front();
    view.focus();
    return true;
}

extern "C" fn view_focus(view: WlcView, focused: bool) {
    view.set_state(VIEW_ACTIVATED, focused);
}

fn main() {
    callback::view_created(view_created);
    callback::view_focus(view_focus);

    // The default log handler will print wlc logs to stdout
    rustwlc::log_set_default_handler();
    let run_fn = rustwlc::init().expect("Unable to initalize!");
    run_fn();
}
