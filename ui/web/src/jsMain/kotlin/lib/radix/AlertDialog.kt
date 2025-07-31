@file:JsModule("@radix-ui/react-alert-dialog") @file:JsNonModule

package lib.radix

import react.ComponentType
import web.events.Event
import web.keyboard.KeyboardEvent

/*

Examples
Close after asynchronous form submission
Use the controlled props to programmatically close the Alert Dialog after an async operation has completed.

import * as React from "react";
import { AlertDialog } from "radix-ui";

const wait = () => new Promise((resolve) => setTimeout(resolve, 1000));

export default () => {
	const [open, setOpen] = React.useState(false);

	return (
		<AlertDialog.Root open={open} onOpenChange={setOpen}>
			<AlertDialog.Trigger>Open</AlertDialog.Trigger>
			<AlertDialog.Portal>
				<AlertDialog.Overlay />
				<AlertDialog.Content>
					<form
						onSubmit={(event) => {
							wait().then(() => setOpen(false));
							event.preventDefault();
						}}
					>
						{/** some inputs */}
						<button type="submit">Submit</button>
					</form>
				</AlertDialog.Content>
			</AlertDialog.Portal>
		</AlertDialog.Root>
	);
};
Custom portal container
Customise the element that your alert dialog portals into.

export default () => {
	const [container, setContainer] = React.useState(null);
	return (
		<div>
			<AlertDialog.Root>
				<AlertDialog.Trigger />
				<AlertDialog.Portal container={container}>
					<AlertDialog.Overlay />
					<AlertDialog.Content>...</AlertDialog.Content>
				</AlertDialog.Portal>
			</AlertDialog.Root>

			<div ref={setContainer} />
		</div>
	);
};

*/

/**
 * @see AlertDialogPrimitiveRoot
 */
external interface AlertRootProps : RadixProps {
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
}

/**
 * Contains all the parts of an alert dialog.
 */
@JsName("Root")
external val AlertDialogPrimitiveRoot: ComponentType<AlertRootProps>

/**
 * @see AlertDialogPrimitiveTrigger
 */
external interface AlertDialogTriggerProps : RadixProps, PropsWithAsChild {
   @JsName("data-state")
   var dataState: String? // "open" | "closed"
}

/**
 * A button that opens the dialog.
 */
@JsName("Trigger")
external val AlertDialogPrimitiveTrigger: ComponentType<AlertDialogTriggerProps>

/**
 * @see AlertDialogPrimitivePortal
 */
external interface AlertDialogPortalProps : RadixProps {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. If used on this part, it will be
    * inherited by AlertDialogOverlay and AlertDialogContent.
    */
   var forceMount: Boolean?

   /**
    * Specify a container element to portal the content into.
    */
   var container: dynamic
}

/**
 * When used, portals your overlay and content parts into the body.
 */
@JsName("Portal")
external val AlertDialogPrimitivePortal: ComponentType<AlertDialogPortalProps>

/**
 * @see AlertDialogPrimitiveOverlay
 */
external interface AlertDialogOverlayProps : RadixProps, PropsWithAsChild {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. It inherits from AlertDialog.Portal.
    */
   var forceMount: Boolean?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"
}

/**
 * A layer that covers the inert portion of the view when the dialog is open.
 */
@JsName("Overlay")
external val AlertDialogPrimitiveOverlay: ComponentType<AlertDialogOverlayProps>

/**
 * @see AlertDialogPrimitiveContent
 */
external interface AlertDialogContentProps : RadixProps, PropsWithAsChild {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. It inherits from AlertDialogPortal.
    */
   var forceMount: Boolean?

   /**
    * Event handler called when focus moves to the destructive action after opening.
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

   @JsName("data-state")
   var dataState: String?
}

/**
 * Contains content to be rendered when the dialog is open.
 */
@JsName("Content")
external val AlertDialogPrimitiveContent: ComponentType<AlertDialogContentProps>

/**
 * @see AlertDialogPrimitiveCancel
 */
external interface AlertDialogCancelProps : RadixProps, PropsWithAsChild

/**
 * A button that closes the dialog. This button should be distinguished
 * visually from AlertDialog.Action buttons.
 */
@JsName("Cancel")
external val AlertDialogPrimitiveCancel: ComponentType<AlertDialogCancelProps>

/**
 * @see AlertDialogPrimitiveAction
 */
external interface AlertDialogActionProps : RadixProps, PropsWithAsChild

/**
 * A button that closes the dialog. These buttons should be distinguished
 * visually from the AlertDialogCancel button.
 */
@JsName("Action")
external val AlertDialogPrimitiveAction: ComponentType<AlertDialogActionProps>

/**
 * @see AlertDialogPrimitiveTitle
 */
external interface AlertDialogTitleProps : RadixProps, PropsWithAsChild

/**
 * An accessible name to be announced when the dialog is opened. Alternatively,
 * you can provide aria-label or aria-labelledby to AlertDialog.Content and
 * exclude this component.
 */
@JsName("Title")
external val AlertDialogPrimitiveTitle: ComponentType<AlertDialogTitleProps>

/**
 * @see AlertDialogPrimitiveDescription
 */
external interface AlertDialogDescriptionProps : RadixProps, PropsWithAsChild

/**
 * An accessible description to be announced when the dialog is opened. Alternatively,
 * you can provide aria-describedby to AlertDialogContent and exclude this component.
 */
@JsName("Description")
external val AlertDialogPrimitiveDescription: ComponentType<AlertDialogDescriptionProps>


