#![allow(non_snake_case)]
#[macro_use]
extern crate objc;

use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyAccessory,
    NSWindow, NSWindowStyleMask, NSWindowCollectionBehavior,
    NSBackingStoreType,
};
use objc::runtime::{Class, Object, Sel};
use objc::declare::ClassDecl;
use cocoa::base::{nil, id, YES, NO};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString};
use std::sync::Once;

#[allow(non_upper_case_globals)]
const kCGFloatingWindowLevel: i64 = 2147483631;

// NSWindowSharingType values
#[allow(non_upper_case_globals)]
const NSWindowSharingNone: u64 = 0;

// NSBezelStyle values
#[allow(non_upper_case_globals)]
const NSBezelStyleRounded: u64 = 1;

static REGISTER_BUTTON_HANDLER: Once = Once::new();
static REGISTER_DRAGGABLE_VIEW: Once = Once::new();

// Button action handler for close button
extern "C" fn close_button_clicked(_this: &Object, _cmd: Sel, _sender: id) {
    unsafe {
        let app = NSApp();
        let _: () = msg_send![app, terminate: nil];
    }
}

// Button action handler for center test button
extern "C" fn test_button_clicked(_this: &Object, _cmd: Sel, _sender: id) {
    println!("Test button clicked!");
}

// DraggableView mouse event handlers
extern "C" fn mouse_down(this: &Object, _cmd: Sel, event: id) {
    unsafe {
        let window: id = msg_send![this, window];
        let _: () = msg_send![window, performWindowDragWithEvent: event];
    }
}

extern "C" fn accepts_first_mouse(_this: &Object, _cmd: Sel, _event: id) -> bool {
    true // Accept first mouse click without activating
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
                accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> bool,
            );
        }
        
        decl.register();
    });
}

fn main() {
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);

        // Register our custom classes
        register_button_handler_class();
        register_draggable_view_class();

        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);

        let frame = NSRect::new(
            NSPoint::new(200.0, 200.0),
            NSSize::new(400.0, 300.0),
        );

        // Create an NSPanel instead of NSWindow for non-activating behavior
        let panel_class = Class::get("NSPanel").unwrap();
        let window: id = msg_send![panel_class, alloc];
        let window: id = msg_send![window, 
            initWithContentRect:frame 
            styleMask:NSWindowStyleMask::NSBorderlessWindowMask 
            backing:NSBackingStoreType::NSBackingStoreBuffered 
            defer:NO
        ];

        let ns_color_class = Class::get("NSColor").unwrap();
        let black_color: id = msg_send![ns_color_class, blackColor];
        let _: () = msg_send![window, setBackgroundColor: black_color];
        let _: () = msg_send![window, setOpaque: YES];
        let _: () = msg_send![window, setHasShadow: NO];
        let _: () = msg_send![window, setLevel: kCGFloatingWindowLevel];
        
        // Make window invisible in screen recording and screen sharing
        let _: () = msg_send![window, setSharingType: NSWindowSharingNone];
        
        // Make window non-activating - clicks won't steal focus from other apps
        let _: () = msg_send![window, setFloatingPanel: YES];
        let _: () = msg_send![window, setBecomesKeyOnlyIfNeeded: YES];
        
        // Set collection behavior to appear on all spaces
        let behavior = NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary;
        let _: () = msg_send![window, setCollectionBehavior: behavior];
        
        // Enable window to be movable by dragging background
        let _: () = msg_send![window, setMovableByWindowBackground: YES];

        // Create a draggable view as the content view
        let draggable_class = Class::get("DraggableView").unwrap();
        let content_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0));
        let draggable_view: id = msg_send![draggable_class, alloc];
        let draggable_view: id = msg_send![draggable_view, initWithFrame: content_frame];
        let _: () = msg_send![window, setContentView: draggable_view];

        // Create button handler instance
        let handler_class = Class::get("ButtonHandler").unwrap();
        let handler: id = msg_send![handler_class, new];

        // Create Close button (top-right corner)
        let button_class = Class::get("NSButton").unwrap();
        let close_button: id = msg_send![button_class, alloc];
        let close_button_frame = NSRect::new(NSPoint::new(340.0, 260.0), NSSize::new(50.0, 30.0));
        let close_button: id = msg_send![close_button, initWithFrame: close_button_frame];
        let close_title = NSString::alloc(nil).init_str("Close");
        let _: () = msg_send![close_button, setTitle: close_title];
        let _: () = msg_send![close_button, setBezelStyle: NSBezelStyleRounded];
        let _: () = msg_send![close_button, setTarget: handler];
        let _: () = msg_send![close_button, setAction: sel!(closeButtonClicked:)];
        let _: () = msg_send![draggable_view, addSubview: close_button];

        // Create Test button (center)
        let test_button: id = msg_send![button_class, alloc];
        let test_button_frame = NSRect::new(NSPoint::new(150.0, 135.0), NSSize::new(100.0, 30.0));
        let test_button: id = msg_send![test_button, initWithFrame: test_button_frame];
        let test_title = NSString::alloc(nil).init_str("Test");
        let _: () = msg_send![test_button, setTitle: test_title];
        let _: () = msg_send![test_button, setBezelStyle: NSBezelStyleRounded];
        let _: () = msg_send![test_button, setTarget: handler];
        let _: () = msg_send![test_button, setAction: sel!(testButtonClicked:)];
        let _: () = msg_send![draggable_view, addSubview: test_button];

        let _: () = msg_send![window, center];
        let _: () = msg_send![window, orderFrontRegardless];

        app.run();
    }
}
