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
use objc::runtime::{Class, Object, Sel, BOOL};
use objc::declare::ClassDecl;
use std::sync::Once;
use std::sync::atomic::{AtomicPtr, AtomicBool, AtomicU64, Ordering};
use std::thread;

// ---------------- GLOBAL STATE ----------------

static TEXT_FIELD: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());
static TEXT_FIELD_ACTIVE: AtomicBool = AtomicBool::new(false);
static DESKTOP_CHANGE_COUNT: AtomicU64 = AtomicU64::new(0);

const kCGFloatingWindowLevel: i64 = 2147483631;
const NSWindowSharingNone: u64 = 0;
const NSBezelStyleRounded: u64 = 1;
const NSEventTypeKeyDown: u64 = 10;

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

const NSFocusRingTypeNone: u64 = 1;

// NSWindowStyleMask values - NSNonactivatingPanelMask is the KEY!
const NSNonactivatingPanelMask: u64 = 1 << 7; // 128 - prevents activation when clicking

static REGISTER_BUTTON_HANDLER: Once = Once::new();
static REGISTER_DRAGGABLE_VIEW: Once = Once::new();
static REGISTER_FOCUSLESS_TEXT_FIELD: Once = Once::new();
static REGISTER_NON_ACTIVATING_PANEL: Once = Once::new();
static REGISTER_SPACE_CHANGE_HANDLER: Once = Once::new();

// ---------------- BUTTON HANDLERS ----------------

extern "C" fn close_button_clicked(_this: &Object, _cmd: Sel, _sender: id) {
    unsafe {
        let app = NSApp();
        let _: () = msg_send![app, terminate: nil];
    }
}

extern "C" fn test_button_clicked(_this: &Object, _cmd: Sel, _sender: id) {
    println!("Test button clicked!");
}

// ---------------- SPACE/DESKTOP CHANGE HANDLER ----------------

extern "C" fn space_did_change(_this: &Object, _cmd: Sel, _notification: id) {
    let count = DESKTOP_CHANGE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    println!("Desktop/Space changed! Count: {}", count);
    
    thread::spawn(move || {
        println!("[Thread {}] Handling desktop change...", count);
        std::thread::sleep(std::time::Duration::from_millis(100));
        println!("[Thread {}] Desktop change handled.", count);
    });
}

// ---------------- NON-ACTIVATING PANEL ----------------

extern "C" fn panel_can_become_key_window(_this: &Object, _cmd: Sel) -> BOOL {
    NO // Never become key window - prevents focus stealing
}

extern "C" fn panel_can_become_main_window(_this: &Object, _cmd: Sel) -> BOOL {
    NO // Never become main window
}

// ---------------- DRAGGABLE BACKGROUND ----------------

extern "C" fn mouse_down(this: &Object, _cmd: Sel, event: id) {
    unsafe {
        TEXT_FIELD_ACTIVE.store(false, Ordering::SeqCst);
        println!("Text field DEACTIVATED");

        let tf = TEXT_FIELD.load(Ordering::SeqCst);
        if !tf.is_null() {
            let white: id = msg_send![class!(NSColor), whiteColor];
            let _: () = msg_send![tf, setBackgroundColor: white];
        }

        let window: id = msg_send![this, window];
        let _: () = msg_send![window, performWindowDragWithEvent: event];
    }
}

extern "C" fn accepts_first_mouse(_this: &Object, _cmd: Sel, _event: id) -> BOOL {
    YES
}

extern "C" fn view_accepts_first_responder(_this: &Object, _cmd: Sel) -> BOOL {
    NO
}

// ---------------- FOCUSLESS TEXT FIELD ----------------

extern "C" fn text_field_accepts_first_mouse(_this: &Object, _cmd: Sel, _event: id) -> BOOL {
    YES
}

extern "C" fn text_field_mouse_down(_this: &Object, _cmd: Sel, _event: id) {
    TEXT_FIELD_ACTIVE.store(true, Ordering::SeqCst);
    println!("Text field ACTIVATED");

    unsafe {
        let tf = TEXT_FIELD.load(Ordering::SeqCst);
        if !tf.is_null() {
            let active_color: id = msg_send![class!(NSColor), colorWithRed:0.9_f64 green:0.95_f64 blue:1.0_f64 alpha:1.0_f64];
            let _: () = msg_send![tf, setBackgroundColor: active_color];
        }
    }
}

extern "C" fn text_field_accepts_first_responder(_this: &Object, _cmd: Sel) -> BOOL {
    NO
}

extern "C" fn text_field_becomes_first_responder(_this: &Object, _cmd: Sel) -> BOOL {
    NO
}

// ---------------- CLASS REGISTRATION ----------------

fn register_non_activating_panel_class() {
    REGISTER_NON_ACTIVATING_PANEL.call_once(|| {
        let superclass = Class::get("NSPanel").unwrap();
        let mut decl = ClassDecl::new("NonActivatingPanel", superclass).unwrap();

        unsafe {
            decl.add_method(
                sel!(canBecomeKeyWindow),
                panel_can_become_key_window as extern "C" fn(&Object, Sel) -> BOOL,
            );
            decl.add_method(
                sel!(canBecomeMainWindow),
                panel_can_become_main_window as extern "C" fn(&Object, Sel) -> BOOL,
            );
        }

        decl.register();
    });
}

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
                accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> BOOL,
            );
            decl.add_method(
                sel!(acceptsFirstResponder),
                view_accepts_first_responder as extern "C" fn(&Object, Sel) -> BOOL,
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
                text_field_accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> BOOL,
            );
            decl.add_method(
                sel!(mouseDown:),
                text_field_mouse_down as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(acceptsFirstResponder),
                text_field_accepts_first_responder as extern "C" fn(&Object, Sel) -> BOOL,
            );
            decl.add_method(
                sel!(becomeFirstResponder),
                text_field_becomes_first_responder as extern "C" fn(&Object, Sel) -> BOOL,
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

        // Register all custom classes
        register_non_activating_panel_class();
        register_button_handler_class();
        register_draggable_view_class();
        register_focusless_text_field_class();
        register_space_change_handler_class();

        // Set up Desktop/Space change notification observer
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
        println!("Desktop/Space change observer registered.");

        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);

        let frame = NSRect::new(
            NSPoint::new(200.0, 200.0),
            NSSize::new(400.0, 300.0),
        );

        // Use NonActivatingPanel with NSNonactivatingPanelMask - like WS_EX_NOACTIVATE on Windows
        let panel_class = Class::get("NonActivatingPanel").unwrap();
        let window: id = msg_send![panel_class, alloc];
        
        // CRITICAL: NSNonactivatingPanelMask (128) prevents the panel from activating the application
        let style_mask = NSWindowStyleMask::NSBorderlessWindowMask.bits() | NSNonactivatingPanelMask;
        
        let window: id = msg_send![window,
            initWithContentRect:frame
            styleMask:style_mask
            backing:NSBackingStoreType::NSBackingStoreBuffered
            defer:NO
        ];

        let ns_color_class = Class::get("NSColor").unwrap();
        let black_color: id = msg_send![ns_color_class, blackColor];
        let _: () = msg_send![window, setBackgroundColor: black_color];
        let _: () = msg_send![window, setOpaque: YES];
        let _: () = msg_send![window, setHasShadow: NO];
        let _: () = msg_send![window, setLevel: kCGFloatingWindowLevel];
        let _: () = msg_send![window, setSharingType: NSWindowSharingNone];
        
        // Critical panel settings for non-activation
        let _: () = msg_send![window, setFloatingPanel: YES];
        let _: () = msg_send![window, setBecomesKeyOnlyIfNeeded: YES];
        let _: () = msg_send![window, setHidesOnDeactivate: NO];

        let behavior = NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary;

        let _: () = msg_send![window, setCollectionBehavior: behavior];
        let _: () = msg_send![window, setMovableByWindowBackground: YES];

        // Create draggable content view
        let draggable_class = Class::get("DraggableView").unwrap();
        let content_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0));
        let draggable_view: id = msg_send![draggable_class, alloc];
        let draggable_view: id = msg_send![draggable_view, initWithFrame: content_frame];
        let _: () = msg_send![window, setContentView: draggable_view];

        // Button handler
        let handler_class = Class::get("ButtonHandler").unwrap();
        let handler: id = msg_send![handler_class, new];

        // Use regular NSButton
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
        let _: () = msg_send![close_button, setRefusesFirstResponder: YES];
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
        let _: () = msg_send![test_button, setRefusesFirstResponder: YES];
        let _: () = msg_send![draggable_view, addSubview: test_button];

        // Text field
        let text_field_class = Class::get("FocuslessTextField").unwrap();
        let text_field: id = msg_send![text_field_class, alloc];
        let text_field_frame = NSRect::new(NSPoint::new(20.0, 135.0), NSSize::new(250.0, 30.0));
        let text_field: id = msg_send![text_field, initWithFrame: text_field_frame];

        let placeholder = NSString::alloc(nil).init_str("Click to type...");
        let _: () = msg_send![text_field, setPlaceholderString: placeholder];
        let _: () = msg_send![text_field, setBezeled: YES];
        let _: () = msg_send![text_field, setDrawsBackground: YES];

        let white_color: id = msg_send![ns_color_class, whiteColor];
        let _: () = msg_send![text_field, setBackgroundColor: white_color];
        let _: () = msg_send![text_field, setEditable: NO];
        let _: () = msg_send![text_field, setSelectable: NO];
        let _: () = msg_send![text_field, setFocusRingType: NSFocusRingTypeNone];

        let _: () = msg_send![draggable_view, addSubview: text_field];
        TEXT_FIELD.store(text_field as *mut Object, Ordering::SeqCst);

        // Global key event monitor
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
                        println!("Submitted!");
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

        // Local monitor
        let local_block = block::ConcreteBlock::new(move |event: id| -> id {
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
        let local_block = local_block.copy();
        let _local_monitor: id = msg_send![ns_event_class, addLocalMonitorForEventsMatchingMask:mask handler:&*local_block];

        let _: () = msg_send![window, center];
        let _: () = msg_send![window, orderFrontRegardless];

        println!("App started - NonActivatingPanel with canBecomeKeyWindow=NO");

        app.run();
    }
}
