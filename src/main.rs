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

// ---------------- GLOBAL STATE ----------------

static TEXT_FIELD: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());
static WINDOW: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());
static TEXT_FIELD_ACTIVE: AtomicBool = AtomicBool::new(false);
static DESKTOP_CHANGE_COUNT: AtomicU64 = AtomicU64::new(0);

// Window levels
const kCGFloatingWindowLevel: i64 = 2147483631;
const kCGScreenSaverWindowLevel: i64 = 2147483647 - 1;
// CRITICAL FIX: kCGScreenSaverWindowLevel + 2 overflows i32!
// Use kCGMaximumWindowLevel (2147483647) which is the absolute max for i32.
const WINDOW_LEVEL: i64 = 2147483647;  

// CGS Private API - same APIs SEB uses (from CGSPrivate.h)
// These bypass NSWindow setLevel: swizzling!
static WINDOW_NUMBER: AtomicU64 = AtomicU64::new(0);

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGSMainConnectionID() -> u32;
    fn CGSSetWindowLevel(cid: u32, wid: u32, level: i32) -> i32;
    fn CGSOrderWindow(cid: u32, wid: u32, mode: i32, relative_to_wid: u32) -> i32;
    fn CGSGetOnScreenWindowList(cid: u32, pid: u32, list: *mut u32, count: *mut i32) -> i32;
}

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
const NSEventTypeKeyDown: u64 = 10;

static REGISTER_BUTTON_HANDLER: Once = Once::new();
static REGISTER_DRAGGABLE_VIEW: Once = Once::new();
static REGISTER_FOCUSLESS_TEXT_FIELD: Once = Once::new();
static REGISTER_SPACE_CHANGE_HANDLER: Once = Once::new();

// ---------------- LOGGING ----------------

fn log_to_file(message: &str) {
    if let Ok(home) = std::env::var("HOME") {
        let log_path = format!("{}/Desktop/ghostmac_log.txt", home);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let log_line = format!("[{}] {}\n", timestamp, message);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = file.write_all(log_line.as_bytes());
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

// ---------------- SPACE CHANGE HANDLER ----------------

extern "C" fn space_did_change(_this: &Object, _cmd: Sel, _notification: id) {
    let count = DESKTOP_CHANGE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    log_to_file(&format!("Desktop/Space changed! Count: {}", count));
    
    unsafe {
        let window = WINDOW.load(Ordering::SeqCst);
        if !window.is_null() {
            let _: () = msg_send![window, setLevel: WINDOW_LEVEL];
            let _: () = msg_send![window, orderFrontRegardless];
            log_to_file(&format!("Window level reasserted to {}", WINDOW_LEVEL));
        }
    }
}

// ---------------- DRAGGABLE BACKGROUND ----------------

extern "C" fn mouse_down(this: &Object, _cmd: Sel, event: id) {
    unsafe {
        TEXT_FIELD_ACTIVE.store(false, Ordering::SeqCst);
        log_to_file("Text field DEACTIVATED");

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
    log_to_file("Text field ACTIVATED");

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
            decl.add_method(
                sel!(closeButtonClicked:),
                close_button_clicked as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(testButtonClicked:),
                test_button_clicked as extern "C" fn(&Object, Sel, id),
            );
        }
        decl.register();
    });
}

fn register_draggable_view_class() {
    REGISTER_DRAGGABLE_VIEW.call_once(|| {
        let superclass = Class::get("NSView").unwrap();
        let mut decl = ClassDecl::new("DraggableView", superclass).unwrap();

        unsafe {
            decl.add_method(
                sel!(mouseDown:),
                mouse_down as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(acceptsFirstMouse:),
                accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> bool,
            );
        }
        decl.register();
    });
}

fn register_focusless_text_field_class() {
    REGISTER_FOCUSLESS_TEXT_FIELD.call_once(|| {
        let superclass = Class::get("NSTextField").unwrap();
        let mut decl = ClassDecl::new("FocuslessTextField", superclass).unwrap();

        unsafe {
            decl.add_method(
                sel!(acceptsFirstMouse:),
                text_field_accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> bool,
            );
            decl.add_method(
                sel!(mouseDown:),
                text_field_mouse_down as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(acceptsFirstResponder),
                text_field_accepts_first_responder as extern "C" fn(&Object, Sel) -> bool,
            );
            decl.add_method(
                sel!(becomeFirstResponder),
                text_field_becomes_first_responder as extern "C" fn(&Object, Sel) -> bool,
            );
        }
        decl.register();
    });
}

fn register_space_change_handler_class() {
    REGISTER_SPACE_CHANGE_HANDLER.call_once(|| {
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new("SpaceChangeHandler", superclass).unwrap();
        unsafe {
            decl.add_method(
                sel!(spaceDidChange:),
                space_did_change as extern "C" fn(&Object, Sel, id),
            );
        }
        decl.register();
    });
}

// ---------------- KEY FILTER ----------------

fn should_pass_through_key(key_code: u16, modifier_flags: u64) -> bool {
    if key_code == kVK_UpArrow
        || key_code == kVK_DownArrow
        || key_code == kVK_LeftArrow
        || key_code == kVK_RightArrow
        || key_code == kVK_Escape
        || key_code == kVK_Tab
    {
        return true;
    }

    if (modifier_flags & NSEventModifierFlagCommand) != 0
        || (modifier_flags & NSEventModifierFlagOption) != 0
        || (modifier_flags & NSEventModifierFlagControl) != 0
    {
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
        register_space_change_handler_class();

        // Set up space change observer
        let space_handler_class = Class::get("SpaceChangeHandler").unwrap();
        let space_handler: id = msg_send![space_handler_class, new];
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let notification_center: id = msg_send![workspace, notificationCenter];
        let notification_name = NSString::alloc(nil).init_str("NSWorkspaceActiveSpaceDidChangeNotification");
        let _: () = msg_send![notification_center,
            addObserver:space_handler
            selector:sel!(spaceDidChange:)
            name:notification_name
            object:nil
        ];
        log_to_file("Space change observer registered");

        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);

        let frame = NSRect::new(
            NSPoint::new(200.0, 200.0),
            NSSize::new(400.0, 300.0),
        );

        let panel_class = Class::get("NSPanel").unwrap();
        let window: id = msg_send![panel_class, alloc];
        let window: id = msg_send![window,
            initWithContentRect:frame
            styleMask:NSWindowStyleMask::NSBorderlessWindowMask
            backing:NSBackingStoreType::NSBackingStoreBuffered
            defer:NO
        ];

        let ns_color_class = Class::get("NSColor").unwrap();
        // Use BRIGHT RED for visibility!
        let red_color: id = msg_send![ns_color_class, redColor];
        let _: () = msg_send![window, setBackgroundColor: red_color];
        let _: () = msg_send![window, setOpaque: YES];
        let _: () = msg_send![window, setHasShadow: NO];
        
        // Use high window level to appear above SEB!
        let _: () = msg_send![window, setLevel: WINDOW_LEVEL];
        log_to_file(&format!("Window level set to {} (kCGScreenSaverWindowLevel+2)", WINDOW_LEVEL));
        
        let _: () = msg_send![window, setSharingType: NSWindowSharingNone];
        let _: () = msg_send![window, setFloatingPanel: YES];
        let _: () = msg_send![window, setBecomesKeyOnlyIfNeeded: YES];
        let _: () = msg_send![window, setHidesOnDeactivate: NO];

        let behavior = NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary;

        let _: () = msg_send![window, setCollectionBehavior: behavior];
        let _: () = msg_send![window, setMovableByWindowBackground: YES];
        
        // Store window for space change handler
        WINDOW.store(window as *mut Object, Ordering::SeqCst);

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

        // Global key event monitor (using NSEvent, not CGEventTap)
        let ns_event_class = Class::get("NSEvent").unwrap();
        let mask: u64 = 1 << NSEventTypeKeyDown;

        let block = block::ConcreteBlock::new(move |event: id| -> id {
            if !TEXT_FIELD_ACTIVE.load(Ordering::SeqCst) {
                return event;
            }

            let key_code: u16 = msg_send![event, keyCode];
            let modifier_flags: u64 = msg_send![event, modifierFlags];

            if should_pass_through_key(key_code, modifier_flags) {
                return event;
            }

            let tf = TEXT_FIELD.load(Ordering::SeqCst);
            if !tf.is_null() {
                let characters: id = msg_send![event, characters];
                if characters != nil {
                    let current_text: id = msg_send![tf, stringValue];
                    let mutable_string: id = msg_send![class!(NSMutableString), alloc];
                    let mutable_string: id = msg_send![mutable_string, initWithString: current_text];

                    if key_code == kVK_Delete {
                        let length: usize = msg_send![mutable_string, length];
                        if length > 0 {
                            let range = cocoa::foundation::NSRange::new((length - 1) as u64, 1);
                            let _: () = msg_send![mutable_string, deleteCharactersInRange: range];
                        }
                    } else if key_code == kVK_Return {
                        let empty = NSString::alloc(nil).init_str("");
                        let _: () = msg_send![tf, setStringValue: empty];
                        return nil;
                    } else {
                        let _: () = msg_send![mutable_string, appendString: characters];
                    }

                    let _: () = msg_send![tf, setStringValue: mutable_string];
                }
            }

            nil
        });
        let block = block.copy();
        let _monitor: id = msg_send![ns_event_class, addGlobalMonitorForEventsMatchingMask:mask handler:&*block];

        let _: () = msg_send![window, center];
        let _: () = msg_send![window, makeKeyAndOrderFront: nil];
        let _: () = msg_send![window, orderFrontRegardless];
        let _: () = msg_send![window, display]; // Force redraw

        // Log exact window frame
        let frame: NSRect = msg_send![window, frame];
        log_to_file(&format!("Window Frame: x={}, y={}, w={}, h={}", 
            frame.origin.x, frame.origin.y, frame.size.width, frame.size.height));
            
        let is_visible: i8 = msg_send![window, isVisible];
        log_to_file(&format!("Window isVisible reported: {}", is_visible));

        log_to_file("App started with window visible");
        
        // Store window number for CGS API calls
        let win_num: i64 = msg_send![window, windowNumber];
        WINDOW_NUMBER.store(win_num as u64, Ordering::SeqCst);
        log_to_file(&format!("Window number stored: {}", win_num));
        
        // Aggressive heartbeat - use CGS private APIs to bypass SEB's setLevel: swizzle
        thread::spawn(|| {
            let mut count: u64 = 0;
            let cgs_connection = unsafe { CGSMainConnectionID() };
            log_to_file(&format!("CGS connection ID: {}", cgs_connection));
            
            loop {
                count += 1;
                let win_num = WINDOW_NUMBER.load(Ordering::SeqCst) as u32;
                
                if win_num > 0 && cgs_connection > 0 {
                    unsafe {
                        // Use CGS private API to set level
                        let level = WINDOW_LEVEL as i32;
                        let _ = CGSSetWindowLevel(cgs_connection, win_num, level);
                        
                        // AGGRESSIVE: Find any window above us and climb over it
                        let mut window_list: [u32; 200] = [0; 200];
                        let mut count_out: i32 = 0;
                        let list_result = CGSGetOnScreenWindowList(cgs_connection, 0, window_list.as_mut_ptr(), &mut count_out);
                        
                        if list_result == 0 && count_out > 0 {
                            // The list is ordered front-to-back.
                            let top_window = window_list[0];
                            
                            // Check if WE are in the list!
                            let mut found_me = false;
                            for i in 0..(count_out as usize) {
                                if window_list[i] == win_num {
                                    found_me = true;
                                    break;
                                }
                            }
                            
                            if count % 10 == 0 {
                                log_to_file(&format!("OnScreen check: Found me? {} | Top window: {}", found_me, top_window));
                            }
                            if top_window != win_num {
                                // Order explicitly above the top window
                                let order_res = CGSOrderWindow(cgs_connection, win_num, 1, top_window);
                                if count % 10 == 0 {
                                    log_to_file(&format!("Ordering above window {}: result={}", top_window, order_res));
                                }
                            }
                        } else {
                            // Fallback: order front relative to everything
                            let _ = CGSOrderWindow(cgs_connection, win_num, 1, 0);
                        }

                        // Also use NSWindow methods as backup
                        let window = WINDOW.load(Ordering::SeqCst);
                        if !window.is_null() {
                            let _: () = msg_send![window, setLevel: WINDOW_LEVEL];
                            let _: () = msg_send![window, orderFrontRegardless];
                        }
                        
                        // Log every 10th heartbeat
                        if count % 10 == 0 {
                            log_to_file(&format!("Heartbeat #{}: CGS active", count));
                        }
                    }
                }
                
                // 200ms interval - very aggressive
                thread::sleep(std::time::Duration::from_millis(200));
            }
        });
        log_to_file("Heartbeat thread started (every 200ms with CGS APIs)");

        app.run();
    }
}
