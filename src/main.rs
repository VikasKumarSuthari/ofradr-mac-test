#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

#[macro_use]
extern crate objc;

use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyAccessory,
    NSWindowStyleMask, NSWindowCollectionBehavior,
    NSBackingStoreType,
};
use cocoa::base::{nil, id, YES, NO};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString};
use objc::runtime::{Class, Object, Sel};
use objc::declare::ClassDecl;
use std::sync::Once;
use std::sync::atomic::{AtomicPtr, AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::fs::OpenOptions;
use std::io::Write;

use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CGEventMask, CGEventField,
};

// ---------------- GLOBAL STATE ----------------

static TEXT_FIELD: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());
static WINDOW_PTR: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());
static WINDOW_NUMBER: AtomicU64 = AtomicU64::new(0);
static TEXT_FIELD_ACTIVE: AtomicBool = AtomicBool::new(false);

// Window levels
// SEB uses NSScreenSaverWindowLevel + 1. We use Max to be safer.
const WINDOW_LEVEL: i64 = 2147483647; // kCGMaximumWindowLevel

const NSWindowSharingNone: u64 = 0;
const NSBezelStyleRounded: u64 = 1;

const kVK_UpArrow: u16 = 0x7E;
const kVK_DownArrow: u16 = 0x7D;
const kVK_LeftArrow: u16 = 0x7B;
const kVK_RightArrow: u16 = 0x7C;
const kVK_Escape: u16 = 0x35;
const kVK_Tab: u16 = 0x30;
const kVK_Return: u16 = 0x24;
const kVK_Delete: u16 = 0x33;

const NSEventModifierFlagCommand: u64 = 1 << 20;
const NSEventModifierFlagOption: u64 = 1 << 19;
const NSEventModifierFlagControl: u64 = 1 << 18;

static REGISTER_BUTTON_HANDLER: Once = Once::new();
static REGISTER_DRAGGABLE_VIEW: Once = Once::new();
static REGISTER_FOCUSLESS_TEXT_FIELD: Once = Once::new();

// CGS Private API
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGSMainConnectionID() -> u32;
    fn CGSSetWindowLevel(cid: u32, wid: u32, level: i32) -> i32;
    fn CGSOrderWindow(cid: u32, wid: u32, mode: i32, relative_to_wid: u32) -> i32;
    fn CGSGetOnScreenWindowList(cid: u32, pid: u32, list: *mut u32, count: *mut i32) -> i32;
}

// ---------------- LOGGING ----------------

fn log_to_file(message: &str) {
    if let Ok(home) = std::env::var("HOME") {
        let log_path = format!("{}/Desktop/ghostmac_log.txt", home);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            let _ = writeln!(file, "[{}] {}", timestamp, message);
        }
    }
    println!("{}", message);
}

// ---------------- BUTTON HANDLERS ----------------

extern "C" fn close_button_clicked(_this: &Object, _cmd: Sel, _sender: id) {
    unsafe {
        let app = NSApp();
        let _: () = msg_send![app, terminate: nil];
    }
}

extern "C" fn test_button_clicked(_this: &Object, _cmd: Sel, _sender: id) {
    log_to_file("Test button clicked!");
}

// ---------------- DRAGGABLE BACKGROUND ----------------

extern "C" fn mouse_down(this: &Object, _cmd: Sel, event: id) {
    unsafe {
        TEXT_FIELD_ACTIVE.store(false, Ordering::SeqCst);
        let tf = TEXT_FIELD.load(Ordering::SeqCst);
        if !tf.is_null() {
            let white: id = msg_send![class!(NSColor), whiteColor];
            let _: () = msg_send![tf, setBackgroundColor: white];
        }

        let window: id = msg_send![this, window];
        let _: () = msg_send![window, performWindowDragWithEvent: event];
    }
}

extern "C" fn accepts_first_mouse(_this: &Object, _cmd: Sel, _event: id) -> bool {
    true
}

// ---------------- FOCUSLESS TEXT FIELD ----------------

extern "C" fn text_field_accepts_first_mouse(_this: &Object, _cmd: Sel, _event: id) -> bool {
    true
}

extern "C" fn text_field_mouse_down(_this: &Object, _cmd: Sel, _event: id) {
    TEXT_FIELD_ACTIVE.store(true, Ordering::SeqCst);
    unsafe {
        let tf = TEXT_FIELD.load(Ordering::SeqCst);
        if !tf.is_null() {
            let gray: id = msg_send![class!(NSColor), lightGrayColor];
            let _: () = msg_send![tf, setBackgroundColor: gray];
        }
    }
}

extern "C" fn text_field_accepts_first_responder(_this: &Object, _cmd: Sel) -> bool {
    false
}

extern "C" fn text_field_becomes_first_responder(_this: &Object, _cmd: Sel) -> bool {
    false
}

// ---------------- CLASS REGISTRATION ----------------

fn register_button_handler_class() {
    REGISTER_BUTTON_HANDLER.call_once(|| {
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new("ButtonHandler", superclass).unwrap();

        unsafe {
            decl.add_method(sel!(closeButtonClicked:), close_button_clicked as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(testButtonClicked:), test_button_clicked as extern "C" fn(&Object, Sel, id));
        }
        decl.register();
    });
}

fn register_draggable_view_class() {
    REGISTER_DRAGGABLE_VIEW.call_once(|| {
        let superclass = Class::get("NSView").unwrap();
        let mut decl = ClassDecl::new("DraggableView", superclass).unwrap();

        unsafe {
            decl.add_method(sel!(mouseDown:), mouse_down as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(acceptsFirstMouse:), accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> bool);
        }
        decl.register();
    });
}

fn register_focusless_text_field_class() {
    REGISTER_FOCUSLESS_TEXT_FIELD.call_once(|| {
        let superclass = Class::get("NSTextField").unwrap();
        let mut decl = ClassDecl::new("FocuslessTextField", superclass).unwrap();

        unsafe {
            decl.add_method(sel!(acceptsFirstMouse:), text_field_accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> bool);
            decl.add_method(sel!(mouseDown:), text_field_mouse_down as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(acceptsFirstResponder), text_field_accepts_first_responder as extern "C" fn(&Object, Sel) -> bool);
            decl.add_method(sel!(becomeFirstResponder), text_field_becomes_first_responder as extern "C" fn(&Object, Sel) -> bool);
        }
        decl.register();
    });
}

// ---------------- KEY FILTER ----------------

fn should_pass_through_key(key_code: u16, modifier_flags: u64) -> bool {
    if key_code == kVK_UpArrow || key_code == kVK_DownArrow || key_code == kVK_LeftArrow || key_code == kVK_RightArrow || key_code == kVK_Escape || key_code == kVK_Tab {
        return true;
    }
    if (modifier_flags & NSEventModifierFlagCommand) != 0 || (modifier_flags & NSEventModifierFlagOption) != 0 || (modifier_flags & NSEventModifierFlagControl) != 0 {
        return true;
    }
    false
}

// ---------------- MAIN ----------------

fn main() {
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);

        register_button_handler_class();
        register_draggable_view_class();
        register_focusless_text_field_class();

        // 1. Rename Process to hide from SEB (Anti-Kill 1)
        let process_info: id = msg_send![class!(NSProcessInfo), processInfo];
        let new_name = NSString::alloc(nil).init_str("mdworker_helper"); 
        let _: () = msg_send![process_info, setProcessName: new_name];
        log_to_file("Process renamed to 'mdworker_helper' to avoid SEB detection");

        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);

        let frame = NSRect::new(NSPoint::new(300.0, 300.0), NSSize::new(400.0, 300.0));

        let panel_class = Class::get("NSPanel").unwrap();
        let window: id = msg_send![panel_class, alloc];
        let window: id = msg_send![window,
            initWithContentRect:frame
            styleMask:NSWindowStyleMask::NSBorderlessWindowMask
            backing:NSBackingStoreType::NSBackingStoreBuffered
            defer:NO
        ];

        let ns_color_class = Class::get("NSColor").unwrap();
        let red_color: id = msg_send![ns_color_class, redColor]; // RED for visibility check
        let _: () = msg_send![window, setBackgroundColor: red_color];
        let _: () = msg_send![window, setOpaque: YES];
        let _: () = msg_send![window, setHasShadow: YES]; // Shadow helps visibility
        
        // 2. Set Maximum Level (Overlay)
        let _: () = msg_send![window, setLevel: WINDOW_LEVEL];
        
        let _: () = msg_send![window, setSharingType: NSWindowSharingNone];
        let _: () = msg_send![window, setFloatingPanel: YES];
        let _: () = msg_send![window, setBecomesKeyOnlyIfNeeded: YES];
        let _: () = msg_send![window, setHidesOnDeactivate: NO]; // Vital for overlay

        let behavior = NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary;

        let _: () = msg_send![window, setCollectionBehavior: behavior];
        let _: () = msg_send![window, setMovableByWindowBackground: YES];

        // Store for Heartbeat
        WINDOW_PTR.store(window as *mut Object, Ordering::SeqCst);
        let win_num: i64 = msg_send![window, windowNumber];
        WINDOW_NUMBER.store(win_num as u64, Ordering::SeqCst);

        let draggable_class = Class::get("DraggableView").unwrap();
        let content_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0));
        let draggable_view: id = msg_send![draggable_class, alloc];
        let draggable_view: id = msg_send![draggable_view, initWithFrame: content_frame];
        let _: () = msg_send![window, setContentView: draggable_view];

        let handler_class = Class::get("ButtonHandler").unwrap();
        let handler: id = msg_send![handler_class, new];
        let button_class = Class::get("NSButton").unwrap();

        // Close button
        let close_button: id = msg_send![button_class, alloc];
        let close_button_frame = NSRect::new(NSPoint::new(340.0, 260.0), NSSize::new(50.0, 30.0));
        let close_button: id = msg_send![close_button, initWithFrame: close_button_frame];
        let close_title = NSString::alloc(nil).init_str("Close");
        let _: () = msg_send![close_button, setTitle: close_title];
        let _: () = msg_send![close_button, setBezelStyle: NSBezelStyleRounded];
        let _: () = msg_send![close_button, setTarget: handler];
        let _: () = msg_send![close_button, setAction: sel!(closeButtonClicked:)];
        let _: () = msg_send![draggable_view, addSubview: close_button];

        // Test button
        let test_button: id = msg_send![button_class, alloc];
        let test_button_frame = NSRect::new(NSPoint::new(280.0, 135.0), NSSize::new(100.0, 30.0));
        let test_button: id = msg_send![test_button, initWithFrame: test_button_frame];
        let test_title = NSString::alloc(nil).init_str("Test");
        let _: () = msg_send![test_button, setTitle: test_title];
        let _: () = msg_send![test_button, setBezelStyle: NSBezelStyleRounded];
        let _: () = msg_send![test_button, setTarget: handler];
        let _: () = msg_send![test_button, setAction: sel!(testButtonClicked:)];
        let _: () = msg_send![draggable_view, addSubview: test_button];

        // Text field
        let text_field_class = Class::get("FocuslessTextField").unwrap();
        let text_field: id = msg_send![text_field_class, alloc];
        let text_field_frame = NSRect::new(NSPoint::new(20.0, 135.0), NSSize::new(250.0, 30.0));
        let text_field: id = msg_send![text_field, initWithFrame: text_field_frame];
        let placeholder = NSString::alloc(nil).init_str("Type here...");
        let _: () = msg_send![text_field, setPlaceholderString: placeholder];
        let _: () = msg_send![text_field, setBezeled: YES];
        let _: () = msg_send![text_field, setDrawsBackground: YES];
        let white_color: id = msg_send![ns_color_class, whiteColor];
        let _: () = msg_send![text_field, setBackgroundColor: white_color];
        let _: () = msg_send![text_field, setEditable: NO];
        let _: () = msg_send![text_field, setSelectable: NO];
        let _: () = msg_send![draggable_view, addSubview: text_field];
        TEXT_FIELD.store(text_field as *mut Object, Ordering::SeqCst);

        // -------- KEY CAPTURE REMOVED FOR STABILITY --------
        // Focusing on visibility above SEB first.
        log_to_file("Keyboard capture disabled to fix build.");

        let _: () = msg_send![window, center];
        let _: () = msg_send![window, orderFrontRegardless];
        let _: () = msg_send![window, makeKeyAndOrderFront: nil]; // Ensuring it shows up

        log_to_file("Window created and ordered front.");

        // 3. CGS Heartbeat (To Fight SEB Overlay)
        thread::spawn(|| {
            let mut count: u64 = 0;
            let cgs_connection = CGSMainConnectionID();
            loop {
                count += 1;
                let win_num = WINDOW_NUMBER.load(Ordering::SeqCst) as u32;
                if win_num > 0 && cgs_connection > 0 {
                    // Try to order above any window
                    let mut window_list: [u32; 100] = [0; 100];
                    let mut out_count: i32 = 0;
                    CGSGetOnScreenWindowList(cgs_connection, 0, window_list.as_mut_ptr(), &mut out_count);
                    
                    if out_count > 0 {
                       let top_win = window_list[0];
                       if top_win != win_num {
                            // CGSOrderWindow(cid, wid, kCGSOrderAbove, relativeToWid)
                            CGSOrderWindow(cgs_connection, win_num, 1, top_win);
                       }
                    } else {
                        // Just order front
                        CGSOrderWindow(cgs_connection, win_num, 1, 0);
                    }
                    
                    // Reassert level
                    CGSSetWindowLevel(cgs_connection, win_num, WINDOW_LEVEL as i32);
                }
                thread::sleep(std::time::Duration::from_millis(250));
            }
        });

        app.run();
    }
}
