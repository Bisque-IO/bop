@file:JsModule("@radix-ui/react-alert-dialog") @file:JsNonModule

package radix.ui

import react.ComponentType
import web.events.Event
import web.uievents.KeyboardEvent

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
 * @see AlertRootDialog
 */
external interface AlertRootProps : DefaultProps {
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
external val AlertRootDialog: ComponentType<AlertRootProps>

/**
 * @see AlertDialogTrigger
 */
external interface AlertDialogTriggerProps : DefaultProps, PropsWithAsChild {
   @JsName("data-state")
   var dataState: String? // "open" | "closed"
}

/**
 * A button that opens the dialog.
 */
@JsName("Trigger")
external val AlertDialogTrigger: ComponentType<AlertDialogTriggerProps>

/**
 * @see AlertDialogPortal
 */
external interface AlertDialogPortalProps : DefaultProps {
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
external val AlertDialogPortal: ComponentType<AlertDialogPortalProps>

/**
 * @see AlertDialogOverlay
 */
external interface AlertDialogOverlayProps : DefaultProps, PropsWithAsChild {
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
external val AlertDialogOverlay: ComponentType<AlertDialogOverlayProps>

/**
 * @see AlertDialogContent
 */
external interface AlertDialogContentProps : DefaultProps, PropsWithAsChild {
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
external val AlertDialogContent: ComponentType<AlertDialogContentProps>

/**
 * @see AlertDialogCancel
 */
external interface AlertDialogCancelProps : DefaultProps, PropsWithAsChild

/**
 * A button that closes the dialog. This button should be distinguished
 * visually from AlertDialog.Action buttons.
 */
@JsName("Cancel")
external val AlertDialogCancel: ComponentType<AlertDialogCancelProps>

/**
 * @see AlertDialogAction
 */
external interface AlertDialogActionProps : DefaultProps, PropsWithAsChild

/**
 * A button that closes the dialog. These buttons should be distinguished
 * visually from the AlertDialogCancel button.
 */
@JsName("Action")
external val AlertDialogAction: ComponentType<AlertDialogActionProps>

/**
 * @see AlertDialogTitle
 */
external interface AlertDialogTitleProps : DefaultProps, PropsWithAsChild

/**
 * An accessible name to be announced when the dialog is opened. Alternatively,
 * you can provide aria-label or aria-labelledby to AlertDialog.Content and
 * exclude this component.
 */
@JsName("Title")
external val AlertDialogTitle: ComponentType<AlertDialogTitleProps>

/**
 * @see AlertDialogDescription
 */
external interface AlertDialogDescriptionProps : DefaultProps, PropsWithAsChild

/**
 * An accessible description to be announced when the dialog is opened. Alternatively,
 * you can provide aria-describedby to AlertDialogContent and exclude this component.
 */
@JsName("Description")
external val AlertDialogDescription: ComponentType<AlertDialogDescriptionProps>


