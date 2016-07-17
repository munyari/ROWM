// ROWM - Really Obnoxious Window Manager
//     Copyright (C) 2016  Panashe M. Fundira

//     This program is free software: you can redistribute it and/or modify
//     it under the terms of the GNU General Public License as published by
//     the Free Software Foundation, either version 3 of the License, or
//     (at your option) any later version.

//     This program is distributed in the hope that it will be useful,
//     but WITHOUT ANY WARRANTY; without even the implied warranty of
//     MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//     GNU General Public License for more details.

//     You should have received a copy of the GNU General Public License
//     along with this program.  If not, see <http://www.gnu.org/licenses/>.
#[macro_use]

extern crate lazy_static;
extern crate rustwlc;

use std::sync::RwLock;
use std::io::Write;
use rustwlc::*;

use std::cmp;

use rustwlc::xkb::keysyms::*;
use rustwlc::xkb::Keysym;

const KEY_DOWN: Keysym = KEY_Down;
const KEY_ESCAPE: Keysym = KEY_Escape;
const KEY_RETURN: Keysym = KEY_Return;
const MOD_SUPER: KeyMod = MOD_MOD4;

struct Compositor {
    pub view: Option<WlcView>,
    pub grab: Point,
    // which edge is being used to resize a window
    pub edges: ResizeEdge
}

impl Compositor {
    fn new() -> Compositor {
        Compositor {
            view: None,
            grab: Point { x: 0, y: 0 },
            edges: ResizeEdge::empty() }
    }
}

lazy_static! {
    static ref COMPOSITOR: RwLock<Compositor> = RwLock::new(Compositor::new());
}

fn start_interactive_action(view: &WlcView, origin: &Point) -> bool {
    {
        let mut comp = COMPOSITOR.write().unwrap();
        if comp.view != None {
            return false;
        }
        comp.grab = origin.clone();
        comp.view = Some(view.clone());
    }

    view.bring_to_front();
    return true;
}

fn start_interactive_move(view: &WlcView, origin: &Point) {
    start_interactive_action(view, origin);
}

fn start_interactive_resize(view: &WlcView, edges: ResizeEdge, origin: &Point) {
    let geometry = match view.get_geometry() {
        None => { return; }
        Some(g) => g,
    };

    if !start_interactive_action(view, origin) {
        return;
    }
    let halfw = geometry.origin.x + geometry.size.w as i32 / 2;
    let halfh = geometry.origin.y + geometry.size.h as i32 / 2;

    {
        let mut comp = COMPOSITOR.write().unwrap();
        comp.edges = edges.clone();
        // not sure what this means
        // I think this means that the edges set is non-empty
        if comp.edges.bits() == 0 {
            let flag_x = if origin.x < halfw {
                // left edge
                RESIZE_LEFT
            } else if origin.x > halfw {
                // right edge
                RESIZE_RIGHT
            } else {
                // empty set of flags
                ResizeEdge::empty()
            };

            let flag_y = if origin.y < halfh {
                RESIZE_TOP
            } else if origin.y > halfh {
                RESIZE_BOTTOM
            } else {
                ResizeEdge::empty()
            };

            comp.edges = flag_x | flag_y;
        }
    }
    view.set_state(VIEW_RESIZING, true);
}

fn stop_interactive_action() {
    let mut comp = COMPOSITOR.write().unwrap();

    match comp.view {
        None => return,
        Some(ref view) =>
            view.set_state(VIEW_RESIZING, false)
    }

    (*comp).view = None;
    comp.edges = ResizeEdge::empty();
}

fn get_topmost_view(output: &WlcOutput, offset: usize) -> Option<WlcView> {
    let views = output.get_views();
    if views.is_empty() { None }
    else {
        Some(views[(views.len() - 1 + offset) % views.len()].clone())
    }
}

fn render_output(output: &WlcOutput) {
    let resolution = output.get_resolution();
    let views = output.get_views();
    if views.is_empty() { return; }

    let mut toggle = false;
    let mut y = 0;
    let w = resolution.w / 2;
    let h = resolution.h / cmp::max((views.len() + 1) / 2, 1) as u32;
    for (i, view) in views.iter().enumerate() {
        view.set_geometry(ResizeEdge::empty(), &Geometry {
            origin: Point { x: if toggle { w as i32 } else { 0 }, y: y },
            size: Size { w: if !toggle && i == views.len() - 1 { resolution.w } 
                            else { w }, h: h }
        });
        toggle = !toggle;
        y = if y > 0 || !toggle { h as i32 } else { 0 };
    }
}

// Handles

extern fn on_output_resolution(output: WlcOutput, _from: &Size, _to: &Size) {
    render_output(&output);
}

extern fn on_view_created(view: WlcView) -> bool {
    view.set_mask(view.get_output().get_mask());
    view.bring_to_front();
    view.focus();
    render_output(&view.get_output());
    true
}

extern fn on_view_destroyed(view: WlcView) {
    if let Some(top_view) = get_topmost_view(&view.get_output(), 0) {
        top_view.focus();
    }
    render_output(&view.get_output());
}

extern fn on_view_focus(view: WlcView, focused: bool) {
    view.set_state(VIEW_ACTIVATED, focused);
}

extern fn on_view_request_move(view: WlcView, origin: &Point) {
    start_interactive_move(&view, origin);
}

extern fn on_view_request_resize(view: WlcView, edges: ResizeEdge, origin: &Point) {
    start_interactive_resize(&view, edges, origin);
}

// TODO: reduce duplication
fn launch_terminal() {
    let term_program = std::env::var("WAYLAND_TERMINAL")
        .unwrap_or(String::from("/usr/bin/urxvt"));
    Command::new("sh")
        .arg("-c")
        .arg(term_program)
        .spawn()
        .expect("Unable to launch terminal");
}

fn launch_finder() {
    let finder_program = std::env::var("WAYLAND_FINDER")
        .unwrap_or(String::from("/usr/bin/dmenu_run"));
    Command::new("sh")
        .arg("-c")
        .arg(finder_program)
        .spawn()
        .expect("Unable to launch finder");
}

extern fn on_keyboard_key(view: WlcView, _time: u32,
                          mods: &KeyboardModifiers, key: u32,
                          state: KeyState) -> bool {
    let sym = input::keyboard::get_keysym_for_key(key, &mods.mods);
    if state == KeyState::Pressed && mods.mods == MOD_SUPER {
        match sym {
            KEY_Q => {
                // not the root window (desktop background) then close the window
                if view.is_window() {
                    view.close();
                }
                true
            }
            KEY_DOWN => {
                view.send_to_back();
                get_topmost_view(&view.get_output(), 0).unwrap().focus();
                true
            }
            KEY_ESCAPE => {
                terminate();
                true
            }
            KEY_RETURN => {
                launch_terminal();
                true
            }
            KEY_P => {
                launch_finder();
                true
            }
            _ => false,
        }
    }
    else {
        false
    }
}

extern fn on_pointer_button(view: WlcView, _time: u32,
                            mods: &KeyboardModifiers, button: u32,
                            state: ButtonState, point: &Point) -> bool {
    if state == ButtonState::Pressed {
        // not the root window (desktop background)
        if view.is_window() && mods.mods.contains(MOD_CTRL) {
            view.focus();
            // The following is is CTRL is being pressed
            if mods.mods.contains(MOD_CTRL) {
                // Button left, we need to include linux/input.h somehow
                if button == 0x110 {
                    start_interactive_move(&view, point);
                }
                if button == 0x111 {
                    start_interactive_resize(&view, ResizeEdge::empty(), point);
                }
            }
        }
    }
    else {
        stop_interactive_action();
    }

    {
        let comp = COMPOSITOR.read().unwrap();
        return comp.view.is_some();
    }
}

extern fn on_pointer_motion(_in_view: WlcView, _time: u32,
                            point: &Point) -> bool {
    rustwlc::input::pointer::set_position(point);
    {
        // read in the IO sense
        let comp = COMPOSITOR.read().unwrap();
        // If the compositior has a view..
        if let Some(ref view) = comp.view {
            // change in x and y
            let dx = point.x - comp.grab.x;
            let dy = point.y - comp.grab.y;
            // geo is the geometry of the compositor's view
            // geomety represents location and size of the view
            let mut geo = view.get_geometry().unwrap().clone();
            if comp.edges.bits() != 0 {
                // minimum size for a view
                let min = Size { w: 80u32, h: 40u32 };
                let mut new_geo = geo.clone();

                if comp.edges.contains(RESIZE_LEFT) {
                    if dx < 0 {
                        new_geo.size.w += dx.abs() as u32;
                    } else {
                        new_geo.size.w -= dx.abs() as u32;
                    }
                    new_geo.origin.x += dx;
                }
                else if comp.edges.contains(RESIZE_RIGHT) {
                    if dx < 0 {
                        new_geo.size.w -= dx.abs() as u32;
                    } else {
                        new_geo.size.w += dx.abs() as u32;
                    }
                }

                if comp.edges.contains(RESIZE_TOP) {
                    if dy < 0 {
                        new_geo.size.h += dy.abs() as u32;
                    } else {
                        new_geo.size.h -= dy.abs() as u32;
                    }
                        new_geo.origin.y += dy;
                }
                else if comp.edges.contains(RESIZE_BOTTOM) {
                    if dy < 0 {
                        new_geo.size.h -= dy.abs() as u32;
                    } else {
                        new_geo.size.h += dy.abs() as u32;
                    }
                }

                // only update if we're not moving into illegal territory
                if new_geo.size.w >= min.w {
                    geo.origin.x = new_geo.origin.x;
                    geo.size.w = new_geo.size.w;
                }

                if new_geo.size.h >= min.h {
                    geo.origin.y = new_geo.origin.y;
                    geo.size.h = new_geo.size.h;
                }

                // set the geometry for the view, and pass the edges changed
                // by interactive resize
                view.set_geometry(comp.edges, &geo);
            }
            else {
                // The window has been moved, rather than resized
                geo.origin.x += dx;
                geo.origin.y += dy;
                view.set_geometry(ResizeEdge::empty(), &geo);
            }
        }
    }

    {
        let mut comp = COMPOSITOR.write().unwrap();
        comp.grab = point.clone();
        return comp.view.is_some();
    }
}

pub extern fn on_view_request_geometry(view: WlcView, geometry: &Geometry) {
    view.set_geometry(EDGE_NONE, geometry);
}

pub extern fn on_view_request_state(view: WlcView, state: ViewState,
                                 handled: bool) {
    view.set_state(state, handled);
}

// a macro for printing to stderr
macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to STDERR");
    } }
);

fn initialize_callbacks() {
    callback::output_resolution(on_output_resolution);
    callback::view_created(on_view_created);
    callback::view_destroyed(on_view_destroyed);
    callback::view_focus(on_view_focus);
    callback::view_request_move(on_view_request_move);
    callback::view_request_resize(on_view_request_resize);
    callback::keyboard_key(on_keyboard_key);
    callback::pointer_button(on_pointer_button);
    callback::pointer_motion(on_pointer_motion);
    callback::view_request_geometry(on_view_request_geometry);
    callback::view_request_state(on_view_request_state);
}

fn print_disclaimer() -> () {
    let disclaimer = "ROWM -  Copyright (C) 2016  Panashe M. Fundira \
    \nThis program comes with ABSOLUTELY NO WARRANTY \
    \nThis is free software, and you are welcome to redistribute it \
    \nunder certain conditions; read LICENSE for details.";
    println!("{}", disclaimer);
}

fn main() {
    println_stderr!("Starting up ROWM");
    // Shouldn't this go on STDERR?
    print_disclaimer();

    initialize_callbacks();

    rustwlc::log_set_default_handler();
    let wlc_event_loop = rustwlc::init().expect("Unable to initialize wlc!");

    wlc_event_loop()
}
