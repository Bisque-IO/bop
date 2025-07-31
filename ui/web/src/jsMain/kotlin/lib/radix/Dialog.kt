@file:JsModule("@radix-ui/react-dialog") @file:JsNonModule

package lib.radix

import react.ComponentType
import web.events.Event
import web.html.HTMLElement
import web.keyboard.KeyboardEvent

/*

Anatomy
Import all parts and piece them together.

import { Dialog } from "radix-ui";

export default () => (
	<Dialog.Root>
		<Dialog.Trigger />
		<Dialog.Portal>
			<Dialog.Overlay />
			<Dialog.Content>
				<Dialog.Title />
				<Dialog.Description />
				<Dialog.Close />
			</Dialog.Content>
		</Dialog.Portal>
	</Dialog.Root>
);

Examples

Close after asynchronous form submission
Use the controlled props to programmatically close the Dialog after an async operation has completed.

import * as React from "react";
import { Dialog } from "radix-ui";

const wait = () => new Promise((resolve) => setTimeout(resolve, 1000));

export default () => {
	const [open, setOpen] = React.useState(false);

	return (
		<Dialog.Root open={open} onOpenChange={setOpen}>
			<Dialog.Trigger>Open</Dialog.Trigger>
			<Dialog.Portal>
				<Dialog.Overlay />
				<Dialog.Content>
					<form
						onSubmit={(event) => {
							wait().then(() => setOpen(false));
							event.preventDefault();
						}}
					>
						{/** some inputs */}
						<button type="submit">Submit</button>
					</form>
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
};
Scrollable overlay
Move the content inside the overlay to render a dialog with overflow.

// index.jsx
import { Dialog } from "radix-ui";
import "./styles.css";

export default () => {
	return (
		<Dialog.Root>
			<Dialog.Trigger />
			<Dialog.Portal>
				<Dialog.Overlay className="DialogOverlay">
					<Dialog.Content className="DialogContent">...</Dialog.Content>
				</Dialog.Overlay>
			</Dialog.Portal>
		</Dialog.Root>
	);
};
/* styles.css */
.DialogOverlay {
	background: rgba(0 0 0 / 0.5);
	position: fixed;
	top: 0;
	left: 0;
	right: 0;
	bottom: 0;
	display: grid;
	place-items: center;
	overflow-y: auto;
}

.DialogContent {
	min-width: 300px;
	background: white;
	padding: 30px;
	border-radius: 4px;
}
Custom portal container
Customise the element that your dialog portals into.

import * as React from "react";
import { Dialog } from "radix-ui";

export default () => {
	const [container, setContainer] = React.useState(null);
	return (
		<div>
			<Dialog.Root>
				<Dialog.Trigger />
				<Dialog.Portal container={container}>
					<Dialog.Overlay />
					<Dialog.Content>...</Dialog.Content>
				</Dialog.Portal>
			</Dialog.Root>

			<div ref={setContainer} />
		</div>
	);
};


Keyboard Interactions

Key	         Description
---------------------------------------------------------------------
Space          Opens/closes the dialog.
Enter          Opens/closes the dialog.
Tab            Moves focus to the next focusable element.
Shift+Tab      Moves focus to the previous focusable element.
Esc            Closes the dialog and moves focus to Dialog.Trigger.

*/

/**
 * @see DialogPrimitiveRoot
 */
external interface DialogRootProps : RadixProps {
   /**
    * The open state of the dialog when it is initially rendered.
    * Use when you do not need to control its open state.
    */
   var defaultOpen: Boolean?

   /**
    * The controlled open state of the dialog. Must be used in conjunction with onOpenChange.
    */
   var open: Boolean?

   /**
    * Event handler called when the open state of the dialog changes.
    */
   var onOpenChange: ((open: Boolean) -> Unit)?

   /**
    * The modality of the dialog. When set to true, interaction with outside elements
    * will be disabled and only dialog content will be visible to screen readers.
    */
   var modal: Boolean? // Optional in Radix v1+
}

/**
 * Contains all the parts of a dialog.
 */
@JsName("Root")
external val DialogPrimitiveRoot: ComponentType<DialogRootProps>

/**
 * @see DialogPrimitiveTrigger
 */
external interface DialogTriggerProps : RadixProps, PropsWithAsChild {
   @JsName("data-state")
   var dataState: String? // "open" | "closed"
}

/**
 * The button that opens the dialog.
 */
@JsName("Trigger")
external val DialogPrimitiveTrigger: ComponentType<DialogTriggerProps>

/**
 * @see DialogPrimitivePortal
 */
external interface DialogPortalProps : RadixProps {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. If used on this part, it will be
    * inherited by DialogOverlay and DialogContent.
    */
   var forceMount: Boolean?

   /**
    * Specify a container element to portal the content into.
    */
   var container: HTMLElement?
}

/**
 * When used, portals your overlay and content parts into the body.
 */
@JsName("Portal")
external val DialogPrimitivePortal: ComponentType<DialogPortalProps>

/**
 * @see DialogPrimitiveOverlay
 */
external interface DialogOverlayProps : RadixProps, PropsWithAsChild {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. It inherits from DialogPortal.
    */
   var forceMount: Boolean?
}

/**
 * A layer that covers the inert portion of the view when the dialog is open.
 */
@JsName("Overlay")
external val DialogPrimitiveOverlay: ComponentType<DialogOverlayProps>

/**
 * @see DialogPrimitiveContent
 */
external interface DialogContentProps : RadixProps, PropsWithAsChild {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. It inherits from DialogPortal.
    */
   var forceMount: Boolean?

   /**
    * Event handler called when focus moves into the component after opening.
    * It can be prevented by calling event.preventDefault.
    */
   var onOpenAutoFocus: ((Event) -> Unit)?

   /**
    * Event handler called when focus moves to the trigger after closing.
    * It can be prevented by calling event.preventDefault.
    */
   var onCloseAutoFocus: ((Event) -> Unit)?

   /**
    * Event handler called when the escape key is down.
    * It can be prevented by calling event.preventDefault.
    */
   var onEscapeKeyDown: ((KeyboardEvent) -> Unit)?

   /**
    * Event handler called when a pointer event occurs outside the bounds of
    * the component. It can be prevented by calling event.preventDefault.
    */
   var onPointerDownOutside: ((Event) -> Unit)?

   /**
    * Event handler called when an interaction (pointer or focus event) happens outside
    * the bounds of the component. It can be prevented by calling event.preventDefault.
    */
   var onInteractOutside: ((Event) -> Unit)?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"
}

/**
 * Contains content to be rendered in the open dialog.
 */
@JsName("Content")
external val DialogPrimitiveContent: ComponentType<DialogContentProps>

/**
 * @see DialogPrimitiveClose
 */
external interface DialogCloseProps : RadixProps, PropsWithAsChild

/**
 * The button that closes the dialog.
 */
@JsName("Close")
external val DialogPrimitiveClose: ComponentType<DialogCloseProps>

/**
 * @see DialogPrimitiveTitle
 */
external interface DialogTitleProps : RadixProps, PropsWithAsChild

/**
 * An accessible title to be announced when the dialog is opened.
 *
 * If you want to hide the title, wrap it inside our Visually Hidden
 * utility like this <VisuallyHidden asChild>.
 */
@JsName("Title")
external val DialogPrimitiveTitle: ComponentType<DialogTitleProps>

/**
 * @see DialogPrimitiveDescription
 */
external interface DialogDescriptionProps : RadixProps, PropsWithAsChild

/**
 * An optional accessible description to be announced when the dialog is opened.
 *
 * If you want to hide the description, wrap it inside our Visually Hidden utility
 * like this <VisuallyHidden asChild>. If you want to remove the description entirely,
 * remove this part and pass aria-describedby={undefined} to DialogContent.
 */
@JsName("Description")
external val DialogPrimitiveDescription: ComponentType<DialogDescriptionProps>
