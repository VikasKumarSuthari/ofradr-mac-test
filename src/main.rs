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

// DYAMINC STRATEGY: No fixed target. Always Top + 1.
// const TARGET_HIGH_LEVEL: i32 = 2005; // REMOVED

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
    fn CGSGetWindowLevel(cid: u32, wid: u32, level: *mut i32) -> i32;
    fn CGSOrderWindow(cid: u32, wid: u32, mode: i32, relative_to_wid: u32) -> i32;
    // CGSGetOnScreenWindowList removed - suspect crash cause
}

// CFArray C functions
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFArrayGetCount(theArray: core_foundation::array::CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(theArray: core_foundation::array::CFArrayRef, idx: isize) -> *const std::ffi::c_void;
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

fn register_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &**s,
                None => "Box<Any>",
            },
        };
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        log_to_file(&format!("CRASH PANIC: {} at {}", msg, location));
    }));
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
            // VISUAL DEBUG MODE: RED BACKGROUND
            // This confirms if the window is present but obscured, or not present.
            let window: id = msg_send![this, window]; // Get the window associated with this view
            let color: id = msg_send![class!(NSColor), colorWithRed:1.0 green:0.0 blue:0.0 alpha:0.5];
            let _: () = msg_send![window, setBackgroundColor: color];
            log_to_file("DraggableView mouse_down: Window background set to red (debug).");
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

// ---------------- SPACE OBSERVER (RESPAWN LOGIC) ----------------

static REGISTER_SPACE_OBSERVER: Once = Once::new();

extern "C" fn space_changed(_this: &Object, _cmd: Sel, _notification: id) {
    log_to_file("Space Change Detected (Observer)! Respawning to jump space...");
    
    if let Ok(exe_path) = std::env::current_exe() {
        // Spawn new instance
        let _ = std::process::Command::new(exe_path).spawn();
    }
    
    // Kill current instance
    std::process::exit(0);
}

fn register_space_observer_class() {
    REGISTER_SPACE_OBSERVER.call_once(|| {
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new("SpaceObserver", superclass).unwrap();

        unsafe {
            decl.add_method(sel!(spaceChanged:), space_changed as extern "C" fn(&Object, Sel, id));
        }
        decl.register();
    });
}

// ---------------- MAIN ----------------

fn main() {
    register_panic_hook(); // Catch crashes

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
        
        // LAYER 100 BATTLE PREP
        let _: () = msg_send![window, setWorksWhenModal: YES];
        // let _: () = msg_send![window, setPreventsApplicationTerminationWhenModal: NO]; // Optional, default is NO

        // respawn on space change logic
        // We define a block or selector to handle the notification
        // For simplicity in Rust/ObjC, we can just use the NotificationCenter with a block?
        // No, loop is CGS based. Space Change is Cocoa.
        // Let's add an observer.
        
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let notification_center: id = msg_send![workspace, notificationCenter];
        
        // Removed invalid block syntax. Using SpaceObserver class instead.
        
        // Convert block to implementation? 
        // Rust closures as blocks are tricky. 
        // Easier: Create a specific Observer Class like `SpaceObserver`.
        
        register_space_observer_class();
        let observer: id = msg_send![class!(SpaceObserver), new];
        let name = NSString::alloc(nil).init_str("NSWorkspaceActiveSpaceDidChangeNotification");
        let _: () = msg_send![notification_center, addObserver:observer selector:sel!(spaceChanged:) name:name object:workspace];


        // SEB Mimicry: Canary in the Coal Mine
        // stationary (16) + aux (256) + disallowTile (2048) + allSpaces (1) + ignoresCycle (4)
        // Bitmask: 1 | 4 | 16 | 256 | 2048 = 2325
        let behavior: cocoa::foundation::NSUInteger = 2325;
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

        // 3. CGS Heartbeat (Find Top Window & Order Above)
        thread::spawn(|| {
            use core_graphics::display::kCGWindowListOptionAll;
            use core_graphics::display::kCGNullWindowID;
            use core_graphics::display::CGWindowListCopyWindowInfo;
            use core_foundation::base::{TCFType, CFType}; // Import CFType struct and TCFType trait
            use core_foundation::number::CFNumber;
            use core_foundation::string::CFString;
            use core_foundation::dictionary::CFDictionary;

            let cgs_connection = unsafe { CGSMainConnectionID() };
            let mut tick_count: u64 = 0;
            
            loop {
                tick_count += 1;
                let should_log = tick_count % 20 == 0;

                // Get our window ID safely
                let my_win_num = WINDOW_NUMBER.load(Ordering::SeqCst) as u32;

                if my_win_num > 0 && cgs_connection > 0 {
                    unsafe {
                        // 1. VISIBILITY CHECK
                        if should_log {
                            let screens: id = msg_send![class!(NSScreen), screens];
                            let screen_count: isize = msg_send![screens, count];
                            log_to_file(&format!("Available screens: {}", screen_count));

                            let main_screen: id = msg_send![class!(NSScreen), mainScreen];
                            let screen_frame: NSRect = msg_send![main_screen, frame];
                            log_to_file(&format!("Main screen frame: {{ x: {}, y: {}, w: {}, h: {} }}", 
                                screen_frame.origin.x, screen_frame.origin.y, screen_frame.size.width, screen_frame.size.height));
                        }

                        // 2. Get Window List (ALL windows, not just on screen, to find Shields)
                        let array = CGWindowListCopyWindowInfo(kCGWindowListOptionAll, kCGNullWindowID);
                        
                        if !array.is_null() {
                            let count = CFArrayGetCount(array);
                            let mut top_window_found = 0;
                            let mut max_layer_found = 0;
                            let mut max_layer_window_id = 0;

                            // 3. Find Max Layer
                            for i in 0..count {
                                let dic_ref = CFArrayGetValueAtIndex(array, i) as core_foundation::dictionary::CFDictionaryRef;
                                let dic: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_get_rule(dic_ref);
                                
                                let k_number = CFString::new("kCGWindowNumber");
                                let k_layer = CFString::new("kCGWindowLayer");

                                let mut current_wid = 0;
                                if let Some(num_obj) = dic.find(&k_number) {
                                    let num_ref = num_obj.as_CFTypeRef() as core_foundation::number::CFNumberRef;
                                    let num = CFNumber::wrap_under_get_rule(num_ref);
                                    if let Some(wid) = num.to_i32() {
                                        current_wid = wid as u32;
                                    }
                                }

                                if let Some(layer_obj) = dic.find(&k_layer) {
                                    let layer_ref = layer_obj.as_CFTypeRef() as core_foundation::number::CFNumberRef;
                                    if let Some(l) = CFNumber::wrap_under_get_rule(layer_ref).to_i32() {
                                        if l > max_layer_found && current_wid != my_win_num {
                                            max_layer_found = l;
                                            max_layer_window_id = current_wid;
                                        }
                                    }
                                }

                                if top_window_found == 0 && current_wid != 0 && current_wid != my_win_num {
                                    top_window_found = current_wid;
                                }
                            }
                            
                            core_foundation::base::CFRelease(array as *const std::ffi::c_void);

                            if should_log {
                                log_to_file(&format!("MAX LAYER FOUND: {} on window {}", max_layer_found, max_layer_window_id));
                            }

                            // 4. CALCULATE LEVEL & BATTLE STRATEGY
                            let top_window_layer = max_layer_found;
                            
                            // STRATEGY: INFINITE ESCALATION (Always +1)
                            // If SEB is at 2005, we go to 2006. If they go to 2006, we go to 2007.
                            
                            let target_level = if top_window_layer >= 100 {
                                top_window_layer + 1 
                            } else {
                                // Minimum floor (ScreenSaver is usually ~2000. Let's aim higher to be safe).
                                2500
                            };

                            // SET LEVEL
                            let level_res = CGSSetWindowLevel(cgs_connection, my_win_num, target_level);
                            
                            // VERIFY LEVEL (Did the OS clamp us?)
                            let mut actual_level: i32 = 0;
                            let _ = CGSGetWindowLevel(cgs_connection, my_win_num, &mut actual_level);
                            
                            if should_log {
                                // Check Visibility State
                                let mut is_visible = false;
                                let mut occlusion_state: u64 = 0;
                                
                                let window_ptr = WINDOW_PTR.load(Ordering::SeqCst);
                                if !window_ptr.is_null() {
                                    let window = window_ptr as id;
                                    is_visible = msg_send![window, isVisible];
                                    occlusion_state = msg_send![window, occlusionState];
                                }
                                
                                let visible_str = if is_visible { "YES" } else { "NO" };
                                let occlusion_str = if occlusion_state & 2 != 0 { "VISIBLE" } else { "OCCLUDED" };

                                if actual_level < target_level && target_level > 100 {
                                     log_to_file(&format!("⚠️ WARNING: Level clamped to {}. Permissions? Vis:{} Occ:{}", actual_level, visible_str, occlusion_str));
                                } else {
                                     log_to_file(&format!("Using Level {} (Actual={}). Top={} Vis:{} Occ:{}", target_level, actual_level, top_window_layer, visible_str, occlusion_str));
                                }
                            }

                            // AGGRESSIVE Z-ORDER SPAM (Works at any level)
                            unsafe {
                                let window_ptr = WINDOW_PTR.load(Ordering::SeqCst);
                                if !window_ptr.is_null() {
                                    let window = window_ptr as id;
                                    
                                    // Spam ordering to win race conditions
                                    for _ in 0..5 {
                                        let _: () = msg_send![window, orderFrontRegardless];
                                        let _: () = msg_send![window, makeKeyAndOrderFront: nil];
                                    }
                                    
                                    // Re-assert behavior
                                    let behavior: cocoa::foundation::NSUInteger = 2325;
                                    let _: () = msg_send![window, setCollectionBehavior: behavior];
                                }
                            }
                        }
                    }
                }
                thread::sleep(std::time::Duration::from_millis(10));
            }
        });

        app.run();
    }
}