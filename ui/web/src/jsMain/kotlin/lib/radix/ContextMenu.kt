@file:JsModule("@radix-ui/react-context-menu") @file:JsNonModule

package lib.radix

import react.*
import web.events.Event
import web.html.HTMLElement
import web.keyboard.KeyboardEvent

/*

Examples

With submenus
You can create submenus by using ContextMenu.Sub in combination with its parts.

<ContextMenu.Root>
	<ContextMenu.Trigger>…</ContextMenu.Trigger>
	<ContextMenu.Portal>
		<ContextMenu.Content>
			<ContextMenu.Item>…</ContextMenu.Item>
			<ContextMenu.Item>…</ContextMenu.Item>
			<ContextMenu.Separator />
			<ContextMenu.Sub>
				<ContextMenu.SubTrigger>Sub menu →</ContextMenu.SubTrigger>
				<ContextMenu.Portal>
					<ContextMenu.SubContent>
						<ContextMenu.Item>Sub menu item</ContextMenu.Item>
						<ContextMenu.Item>Sub menu item</ContextMenu.Item>
						<ContextMenu.Arrow />
					</ContextMenu.SubContent>
				</ContextMenu.Portal>
			</ContextMenu.Sub>
			<ContextMenu.Separator />
			<ContextMenu.Item>…</ContextMenu.Item>
		</ContextMenu.Content>
	</ContextMenu.Portal>
</ContextMenu.Root>


With disabled items
You can add special styles to disabled items via the data-disabled attribute.

// index.jsx
import { ContextMenu } from "radix-ui";
import "./styles.css";

export default () => (
	<ContextMenu.Root>
		<ContextMenu.Trigger>…</ContextMenu.Trigger>
		<ContextMenu.Portal>
			<ContextMenu.Content>
				<ContextMenu.Item className="ContextMenuItem" disabled>
					…
				</ContextMenu.Item>
				<ContextMenu.Item className="ContextMenuItem">…</ContextMenu.Item>
			</ContextMenu.Content>
		</ContextMenu.Portal>
	</ContextMenu.Root>
);
/* styles.css */
.ContextMenuItem[data-disabled] {
	color: gainsboro;
}
With separators
Use the Separator part to add a separator between items.

<ContextMenu.Root>
	<ContextMenu.Trigger>…</ContextMenu.Trigger>
	<ContextMenu.Portal>
		<ContextMenu.Content>
			<ContextMenu.Item>…</ContextMenu.Item>
			<ContextMenu.Separator />
			<ContextMenu.Item>…</ContextMenu.Item>
			<ContextMenu.Separator />
			<ContextMenu.Item>…</ContextMenu.Item>
		</ContextMenu.Content>
	</ContextMenu.Portal>
</ContextMenu.Root>
With labels
Use the Label part to help label a section.

<ContextMenu.Root>
	<ContextMenu.Trigger>…</ContextMenu.Trigger>
	<ContextMenu.Portal>
		<ContextMenu.Content>
			<ContextMenu.Label>Label</ContextMenu.Label>
			<ContextMenu.Item>…</ContextMenu.Item>
			<ContextMenu.Item>…</ContextMenu.Item>
			<ContextMenu.Item>…</ContextMenu.Item>
		</ContextMenu.Content>
	</ContextMenu.Portal>
</ContextMenu.Root>
With checkbox items
Use the CheckboxItem part to add an item that can be checked.

import * as React from "react";
import { CheckIcon } from "@radix-ui/react-icons";
import { ContextMenu } from "radix-ui";

export default () => {
	const [checked, setChecked] = React.useState(true);

	return (
		<ContextMenu.Root>
			<ContextMenu.Trigger>…</ContextMenu.Trigger>
			<ContextMenu.Portal>
				<ContextMenu.Content>
					<ContextMenu.Item>…</ContextMenu.Item>
					<ContextMenu.Item>…</ContextMenu.Item>
					<ContextMenu.Separator />
					<ContextMenu.CheckboxItem
						checked={checked}
						onCheckedChange={setChecked}
					>
						<ContextMenu.ItemIndicator>
							<CheckIcon />
						</ContextMenu.ItemIndicator>
						Checkbox item
					</ContextMenu.CheckboxItem>
				</ContextMenu.Content>
			</ContextMenu.Portal>
		</ContextMenu.Root>
	);
};
With radio items
Use the RadioGroup and RadioItem parts to add an item that can be checked amongst others.

import * as React from "react";
import { CheckIcon } from "@radix-ui/react-icons";
import { ContextMenu } from "radix-ui";

export default () => {
	const [color, setColor] = React.useState("blue");

	return (
		<ContextMenu.Root>
			<ContextMenu.Trigger>…</ContextMenu.Trigger>
			<ContextMenu.Portal>
				<ContextMenu.Content>
					<ContextMenu.RadioGroup value={color} onValueChange={setColor}>
						<ContextMenu.RadioItem value="red">
							<ContextMenu.ItemIndicator>
								<CheckIcon />
							</ContextMenu.ItemIndicator>
							Red
						</ContextMenu.RadioItem>
						<ContextMenu.RadioItem value="blue">
							<ContextMenu.ItemIndicator>
								<CheckIcon />
							</ContextMenu.ItemIndicator>
							Blue
						</ContextMenu.RadioItem>
						<ContextMenu.RadioItem value="green">
							<ContextMenu.ItemIndicator>
								<CheckIcon />
							</ContextMenu.ItemIndicator>
							Green
						</ContextMenu.RadioItem>
					</ContextMenu.RadioGroup>
				</ContextMenu.Content>
			</ContextMenu.Portal>
		</ContextMenu.Root>
	);
};
With complex items
You can add extra decorative elements in the Item parts, such as images.

import { ContextMenu } from "radix-ui";

export default () => (
	<ContextMenu.Root>
		<ContextMenu.Trigger>…</ContextMenu.Trigger>
		<ContextMenu.Portal>
			<ContextMenu.Content>
				<ContextMenu.Item>
					<img src="…" />
					Adolfo Hess
				</ContextMenu.Item>
				<ContextMenu.Item>
					<img src="…" />
					Miyah Myles
				</ContextMenu.Item>
			</ContextMenu.Content>
		</ContextMenu.Portal>
	</ContextMenu.Root>
);
Constrain the content/sub-content size
You may want to constrain the width of the content (or sub-content) so that it
matches the trigger (or sub-trigger) width. You may also want to constrain its
height to not exceed the viewport.

We expose several CSS custom properties such as
--radix-context-menu-trigger-width and
--radix-context-menu-content-available-height to support this.
Use them to constrain the content dimensions.

// index.jsx
import { ContextMenu } from "radix-ui";
import "./styles.css";

export default () => (
	<ContextMenu.Root>
		<ContextMenu.Trigger>…</ContextMenu.Trigger>
		<ContextMenu.Portal>
			<ContextMenu.Content className="ContextMenuContent">
				…
			</ContextMenu.Content>
		</ContextMenu.Portal>
	</ContextMenu.Root>
);
/* styles.css */
.ContextMenuContent {
	width: var(--radix-context-menu-trigger-width);
	max-height: var(--radix-context-menu-content-available-height);
}


Origin-aware animations
We expose a CSS custom property --radix-context-menu-content-transform-origin.
Use it to animate the content from its computed origin based on side,
sideOffset, align, alignOffset and any collisions.

// index.jsx
import { ContextMenu } from "radix-ui";
import "./styles.css";

export default () => (
	<ContextMenu.Root>
		<ContextMenu.Trigger>…</ContextMenu.Trigger>
		<ContextMenu.Portal>
			<ContextMenu.Content className="ContextMenuContent">
				…
			</ContextMenu.Content>
		</ContextMenu.Portal>
	</ContextMenu.Root>
);

// styles.css
.ContextMenuContent {
	transform-origin: var(--radix-context-menu-content-transform-origin);
	animation: scaleIn 0.5s ease-out;
}

@keyframes scaleIn {
	from {
		opacity: 0;
		transform: scale(0);
	}
	to {
		opacity: 1;
		transform: scale(1);
	}
}

Collision-aware animations
We expose data-side and data-align attributes. Their values will change at runtime
to reflect collisions. Use them to create collision and direction-aware animations.

// index.jsx
import { ContextMenu } from "radix-ui";
import "./styles.css";

export default () => (
	<ContextMenu.Root>
		<ContextMenu.Trigger>…</ContextMenu.Trigger>
		<ContextMenu.Portal>
			<ContextMenu.Content className="ContextMenuContent">
				…
			</ContextMenu.Content>
		</ContextMenu.Portal>
	</ContextMenu.Root>
);
// styles.css
.ContextMenuContent {
	animation-duration: 0.6s;
	animation-timing-function: cubic-bezier(0.16, 1, 0.3, 1);
}
.ContextMenuContent[data-side="top"] {
	animation-name: slideUp;
}
.ContextMenuContent[data-side="bottom"] {
	animation-name: slideDown;
}

@keyframes slideUp {
	from {
		opacity: 0;
		transform: translateY(10px);
	}
	to {
		opacity: 1;
		transform: translateY(0);
	}
}

@keyframes slideDown {
	from {
		opacity: 0;
		transform: translateY(-10px);
	}
	to {
		opacity: 1;
		transform: translateY(0);
	}
}

*/

/**
 * @see ContextMenuPrimitiveRoot
 */
external interface ContextMenuRootProps : RadixProps {
   /**
    * The reading direction of submenus when applicable. If omitted, inherits
    * globally from DirectionProvider or assumes LTR (left-to-right) reading mode.
    */
   var dir: String? // "ltr" | "rtl

   /**
    * Event handler called when the open state of the context menu changes.
    */
   var onOpenChange: ((open: Boolean) -> Unit)?

   /**
    * The modality of the context menu. When set to true, interaction with
    * outside elements will be disabled and only menu content will be visible
    * to screen readers.
    */
   var modal: Boolean?
}

/**
 * Contains all the parts of a context menu.
 */
@JsName("Root")
external val ContextMenuPrimitiveRoot: ComponentType<ContextMenuRootProps>

/**
 * @see ContextMenuPrimitiveTrigger
 */
external interface ContextMenuTriggerProps : RadixProps, PropsWithAsChild {
   /**
    * When true, the context menu won't open when right-clicking. Note that this
    * will also restore the native context menu.
    */
   var disabled: Boolean?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"
}

/**
 * The area that opens the context menu. Wrap it around the target you want the context
 * menu to open from when right-clicking (or using the relevant keyboard shortcuts).
 */
@JsName("Trigger")
external val ContextMenuPrimitiveTrigger: ComponentType<ContextMenuTriggerProps>

/**
 * @see ContextMenuPrimitivePortal
 */
external interface ContextMenuPortalProps : RadixProps {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. If used on this part, it will be
    * inherited by ContextMenuContent and ContextMenuSubContent respectively.
    */
   var forceMount: Boolean?

   /**
    * Specify a container element to portal the content into.
    */
   var container: HTMLElement?
}

/**
 * When used, portals the content part into the body.
 */
@JsName("Portal")
external val ContextMenuPrimitivePortal: ComponentType<ContextMenuPortalProps>

/**
 *
 */
external interface ContextMenuContentProps : RadixProps, PropsWithAsChild {
   /**
    * When true, keyboard navigation will loop from last item to first, and vice versa.
    */
   var loop: Boolean?

   /**
    * Event handler called when focus moves back after closing.
    * It can be prevented by calling event.preventDefault.
    */
   var onCloseAutoFocus: ((Event) -> Unit)?

   /**
    * Event handler called when the escape key is down. It can be prevented by
    * calling event.preventDefault.
    */
   var onEscapeKeyDown: ((KeyboardEvent) -> Unit)?

   /**
    * Event handler called when a pointer event occurs outside the bounds of the
    * component. It can be prevented by calling event.preventDefault.
    */
   var onPointerDownOutside: ((Event) -> Unit)?

   /**
    * Event handler called when focus moves outside the bounds of the component.
    * It can be prevented by calling event.preventDefault.
    */
   var onFocusOutside: ((Event) -> Unit)?

   /**
    * Event handler called when an interaction (pointer or focus event) happens
    * outside the bounds of the component. It can be prevented by calling
    * event.preventDefault.
    */
   var onInteractOutside: ((Event) -> Unit)?

   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. It inherits from ContextMenuPortal.
    */
   var forceMount: Boolean?

   /**
    * The vertical distance in pixels from the anchor.
    */
   var alignOffset: Int?

   /**
    * When true, overrides the side andalign preferences to prevent collisions with boundary edges.
    */
   var avoidCollisions: Boolean?

   /**
    * The element used as the collision boundary. By default, this is the viewport,
    * though you can provide additional element(s) to be included in this check.
    */
   var collisionBoundary: CollisionBoundary? /* HTMLElement | Array<HTMLElement> */

   /**
    * The distance in pixels from the boundary edges where collision detection should occur.
    * Accepts a number (same for all sides), or a partial padding object, for example:
    *
    * { top: 20, left: 20 }
    * unsafeJso {
    *    top = 20
    *    left = 20
    * }
    *
    * Int | { top: Int, right: Int, bottom: Int, left: Int }
    */
   var collisionPadding: CollisionPadding? // Int | { top: Int, right: Int, bottom: Int, left: Int }

   /**
    * The sticky behavior on the align axis. "partial" will keep the content in the boundary
    * as long as the trigger is at least partially in the boundary whilst "always" will keep
    * the content in the boundary regardless.
    *
    * enum: "partial" | "always"
    */
   var sticky: String? // "partial" | "always"

   /**
    * Whether to hide the content when the trigger becomes fully occluded.
    */
   var hideWhenDetached: Boolean?

   @JsName("data-state")
   var dataState: String // "open" | "closed"

   // data-side
   @JsName("data-side")
   var dataSide: String? /* "top" | "right" | "bottom" | "left" */

   @JsName("data-align")
   var dataAlign: String? /* "start" | "center" | "end" */
}

/*
CSS Variable	Description
--radix-context-menu-content-transform-origin	The transform-origin computed from the content and arrow positions/offsets
--radix-context-menu-content-available-width	   The remaining width between the trigger and the boundary edge
--radix-context-menu-content-available-height	The remaining height between the trigger and the boundary edge
--radix-context-menu-trigger-width	            The width of the trigger
--radix-context-menu-trigger-height	            The height of the trigger
 */

/**
 * The component that pops out in an open context menu.
 */
@JsName("Content")
external val ContextMenuPrimitiveContent: ComponentType<ContextMenuContentProps>

/**
 * @see ContextMenuPrimitiveArrow
 */
external interface ContextMenuArrowProps : RadixProps, PropsWithAsChild {
   /**
    * The width of the arrow in pixels.
    */
   var width: Int?

   /**
    * The height of the arrow in pixels.
    */
   var height: Int?
}

/**
 * An optional arrow element to render alongside a submenu. This can be used to help
 * visually link the trigger item with the ContextMenu.Content. Must be rendered
 * inside ContextMenuContent.
 */
@JsName("Arrow")
external val ContextMenuPrimitiveArrow: ComponentType<ContextMenuArrowProps>

/**
 * @see ContextMenuPrimitiveItem
 */
external interface ContextMenuItemProps : RadixProps, PropsWithAsChild {
   /**
    * When true, prevents the user from interacting with the item.
    */
   var disabled: Boolean?

   /**
    * Event handler called when the user selects an item (via mouse or keyboard).
    * Calling event.preventDefault in this handler will prevent the context menu
    * from closing when selecting that item.
    */
   var onSelect: ((Event) -> Unit)?

   /**
    * Optional text used for typeahead purposes. By default, the typeahead behavior
    * will use the .textContent of the item. Use this when the content is complex,
    * or you have non-textual content inside.
    */
   var textValue: String?

   @JsName("data-highlighted")
   var dataHighlighted: Boolean?

   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

/**
 * The component that contains the context menu items.
 */
@JsName("Item")
external val ContextMenuPrimitiveItem: ComponentType<ContextMenuItemProps>

/**
 * @see ContextMenuPrimitiveGroup
 */
external interface ContextMenuGroupProps : RadixProps, PropsWithAsChild

/**
 * Used to group multiple ContextMenu.Items.
 */
@JsName("Group")
external val ContextMenuPrimitiveGroup: ComponentType<ContextMenuGroupProps>

/**
 * @see ContextMenuPrimitiveLabel
 */
external interface ContextMenuLabelProps : RadixProps, PropsWithAsChild

/**
 * Used to render a label. It won't be focusable using arrow keys.
 */
@JsName("Label")
external val ContextMenuPrimitiveLabel: ComponentType<ContextMenuLabelProps>

/**
 * @see ContextMenuPrimitiveCheckboxItem
 */
external interface ContextMenuCheckboxItemProps : RadixProps, PropsWithAsChild {
   /**
    * The controlled checked state of the item. Must be used in conjunction with onCheckedChange.
    */
   var checked: dynamic /* Boolean | "indeterminate" */

   /**
    * Event handler called when the checked state changes.
    */
   var onCheckedChange: ((dynamic) -> Unit)?

   /**
    * When true, prevents the user from interacting with the item.
    */
   var disabled: Boolean?

   /**
    * Event handler called when the user selects an item (via mouse or keyboard).
    * Calling event.preventDefault in this handler will prevent the context menu
    * from closing when selecting that item.
    */
   var onSelect: ((Event) -> Unit)?

   /**
    * Optional text used for typeahead purposes. By default, the typeahead behavior
    * will use the .textContent of the item. Use this when the content is complex,
    * or you have non-textual content inside.
    */
   var textValue: String?

   @JsName("data-state")
   var dataState: String? // "checked" | "unchecked" | "indeterminate"

   @JsName("data-highlighted")
   var dataHighlighted: String?

   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

/**
 * An item that can be controlled and rendered like a checkbox.
 */
@JsName("CheckboxItem")
external val ContextMenuPrimitiveCheckboxItem: ComponentType<ContextMenuCheckboxItemProps>

/**
 * @see ContextMenuPrimitiveRadioGroup
 */
external interface ContextMenuRadioGroupProps : PropsWithChildren, PropsWithAsChild, PropsWithValue<String> {
   /**
    * Event handler called when the value changes.
    */
   var onValueChange: ((String) -> Unit)?
}

/**
 * Used to group multiple ContextMenuRadioItems.
 */
@JsName("RadioGroup")
external val ContextMenuPrimitiveRadioGroup: ComponentType<ContextMenuRadioGroupProps>

/**
 * @see ContextMenuPrimitiveRadioItem
 */
external interface ContextMenuRadioItemProps : RadixProps, PropsWithAsChild, PropsWithValue<String> {
   /**
    * When true, prevents the user from interacting with the item.
    */
   var disabled: Boolean?

   /**
    * Event handler called when the user selects an item (via mouse or keyboard).
    * Calling event.preventDefault in this handler will prevent the context menu
    * from closing when selecting that item.
    */
   var onSelect: ((Event) -> Unit)?

   /**
    * Optional text used for typeahead purposes. By default, the typeahead behavior
    * will use the .textContent of the item. Use this when the content is complex,
    * or you have non-textual content inside.
    */
   var textValue: String?

   @JsName("data-state")
   var dataState: String? // "checked" | "unchecked" | "indeterminate"

   @JsName("data-highlighted")
   var dataHighlighted: dynamic

   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

/**
 * An item that can be controlled and rendered like a radio.
 */
@JsName("RadioItem")
external val ContextMenuPrimitiveRadioItem: ComponentType<ContextMenuRadioItemProps>

/**
 * @see ContextMenuPrimitiveItemIndicator
 */
external interface ContextMenuItemIndicatorProps : RadixProps, PropsWithAsChild {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries.
    */
   var forceMount: Boolean?

   @JsName("data-state")
   var dataState: String? // "checked" | "unchecked" | "indeterminate"
}

/**
 * Renders when the parent ContextMenu.CheckboxItem or ContextMenu.RadioItem is checked.
 * You can style this element directly, or you can use it as a wrapper to put an icon
 * into, or both.
 */
@JsName("ItemIndicator")
external val ContextMenuPrimitiveItemIndicator: ComponentType<ContextMenuItemIndicatorProps>

/**
 * @see ContextMenuPrimitiveSeparator
 */
external interface ContextMenuSeparatorProps : RadixProps, PropsWithAsChild

/**
 * Used to visually separate items in the context menu.
 */
@JsName("Separator")
external val ContextMenuPrimitiveSeparator: ComponentType<ContextMenuSeparatorProps>

/**
 * @see ContextMenuPrimitiveSub
 */
external interface ContextMenuSubProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   /**
    * The open state of the submenu when it is initially rendered.
    * Use when you do not need to control its open state.
    */
   var defaultOpen: Boolean?

   /**
    * The controlled open state of the submenu. Must be used in conjunction with onOpenChange.
    */
   var open: Boolean?

   /**
    * Event handler called when the open state of the submenu changes.
    */
   var onOpenChange: ((open: Boolean) -> Unit)?
}

/**
 * Contains all the parts of a submenu.
 */
@JsName("Sub")
external val ContextMenuPrimitiveSub: ComponentType<ContextMenuSubProps>

/**
 * @see ContextMenuPrimitiveSubTrigger
 */
external interface ContextMenuSubTriggerProps : RadixProps, PropsWithAsChild {
   /**
    * When true, prevents the user from interacting with the item.
    */
   var disabled: Boolean?

   /**
    * Optional text used for typeahead purposes. By default, the typeahead behavior
    * will use the .textContent of the item. Use this when the content is complex,
    * or you have non-textual content inside.
    */
   var textValue: String?

   @JsName("data-state")
   var dataState: String? // "checked" | "unchecked" | "indeterminate"

   @JsName("data-highlighted")
   var dataHighlighted: dynamic

   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

/**
 * An item that opens a submenu. Must be rendered inside ContextMenuSub.
 */
@JsName("SubTrigger")
external val ContextMenuPrimitiveSubTrigger: ComponentType<ContextMenuSubTriggerProps>

/**
 * @see ContextMenuPrimitiveSubContent
 */
external interface ContextMenuSubContentProps : RadixProps, PropsWithAsChild {
   /**
    * When true, keyboard navigation will loop from last item to first, and vice versa.
    */
   var loop: Boolean?

   /**
    * Event handler called when the escape key is down. It can be prevented by
    * calling event.preventDefault
    */
   var onEscapeKeyDown: ((KeyboardEvent) -> Unit)?

   /**
    * Event handler called when a pointer event occurs outside the bounds of the
    * component. It can be prevented by calling event.preventDefault.
    */
   var onPointerDownOutside: ((Event) -> Unit)?

   /**
    * Event handler called when focus moves outside the bounds of the component.
    * It can be prevented by calling event.preventDefault.
    */
   var onFocusOutside: ((Event) -> Unit)?

   /**
    * Event handler called when an interaction (pointer or focus event) happens
    * outside the bounds of the component. It can be prevented by calling
    * event.preventDefault.
    */
   var onInteractOutside: ((Event) -> Unit)?

   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. It inherits from ContextMenuPortal.
    */
   var forceMount: Boolean?

   /**
    * The distance in pixels from the trigger.
    */
   var sideOffset: Int?

   /**
    * An offset in pixels from the "start" or "end" alignment options.
    */
   var alignOffset: Int?

   /**
    * When true, overrides the side andalign preferences to prevent collisions with boundary edges.
    */
   var avoidCollisions: Boolean?

   /**
    * The element used as the collision boundary. By default, this is the viewport,
    * though you can provide additional element(s) to be included in this check.
    */
   var collisionBoundary: CollisionBoundary? // HTMLElement | Array<HTMLElement>

   /**
    * The distance in pixels from the boundary edges where collision detection should occur.
    * Accepts a number (same for all sides), or a partial padding object, for example:
    *
    * { top: 20, left: 20 }
    *
    * unsafeJso {
    *    top = 20
    *    left = 20
    * }
    *
    * Int | { top: Int, right: Int, bottom: Int, left: Int }
    */
   var collisionPadding: CollisionPadding? // Int | { top: Int, right: Int, bottom: Int, left: Int }

   /**
    * The padding between the arrow and the edges of the content. If your content has border-radius,
    * this will prevent it from overflowing the corners.
    */
   var arrowPadding: Int?

   /**
    * The sticky behavior on the align axis. "partial" will keep the content in the boundary as
    * long as the trigger is at least partially in the boundary whilst "always" will keep the
    * content in the boundary regardless.
    *
    * enum "partial" | "always"
    */
   var sticky: String? // "partial" | "always"

   /**
    * Whether to hide the content when the trigger becomes fully occluded.
    */
   var hideWhenDetached: Boolean?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"

   @JsName("data-side")
   var side: String? // "left" | "right" | "bottom" | "top"

   @JsName("data-align")
   var align: String? // "start" | "center" | "end"
}

/*

CSS Variable	                                 Description
--radix-context-menu-content-transform-origin	The transform-origin computed from the content and arrow positions/offsets
--radix-context-menu-content-available-width	   The remaining width between the trigger and the boundary edge
--radix-context-menu-content-available-height	The remaining height between the trigger and the boundary edge
--radix-context-menu-trigger-width	            The width of the trigger
--radix-context-menu-trigger-height	            The height of the trigger

*/

/**
 * The component that pops out when a submenu is open. Must be rendered inside ContextMenuSub.
 */
@JsName("SubContent")
external val ContextMenuPrimitiveSubContent: ComponentType<ContextMenuSubContentProps>

