@file:JsModule("@radix-ui/react-menubar")
@file:JsNonModule

package lib.radix

import react.ComponentType
import web.events.Event
import web.html.HTMLElement
import web.uievents.KeyboardEvent

/*

Anatomy
Import all parts and piece them together.

import { Menubar } from "radix-ui";

export default () => (
	<Menubar.Root>
		<Menubar.Menu>
			<Menubar.Trigger />
			<Menubar.Portal>
				<Menubar.Content>
					<Menubar.Label />
					<Menubar.Item />

					<Menubar.Group>
						<Menubar.Item />
					</Menubar.Group>

					<Menubar.CheckboxItem>
						<Menubar.ItemIndicator />
					</Menubar.CheckboxItem>

					<Menubar.RadioGroup>
						<Menubar.RadioItem>
							<Menubar.ItemIndicator />
						</Menubar.RadioItem>
					</Menubar.RadioGroup>

					<Menubar.Sub>
						<Menubar.SubTrigger />
						<Menubar.Portal>
							<Menubar.SubContent />
						</Menubar.Portal>
					</Menubar.Sub>

					<Menubar.Separator />
					<Menubar.Arrow />
				</Menubar.Content>
			</Menubar.Portal>
		</Menubar.Menu>
	</Menubar.Root>
);

Examples
With submenus
You can create submenus by using Menubar.Sub in combination with its parts.

<Menubar.Root>
	<Menubar.Menu>
		<Menubar.Trigger>…</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content>
				<Menubar.Item>…</Menubar.Item>
				<Menubar.Item>…</Menubar.Item>
				<Menubar.Separator />
				<Menubar.Sub>
					<Menubar.SubTrigger>Sub menu →</Menubar.SubTrigger>
					<Menubar.Portal>
						<Menubar.SubContent>
							<Menubar.Item>Sub menu item</Menubar.Item>
							<Menubar.Item>Sub menu item</Menubar.Item>
							<Menubar.Arrow />
						</Menubar.SubContent>
					</Menubar.Portal>
				</Menubar.Sub>
				<Menubar.Separator />
				<Menubar.Item>…</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>
</Menubar.Root>
With disabled items
You can add special styles to disabled items via the data-disabled attribute.

// index.jsx
import { Menubar } from "radix-ui";
import "./styles.css";

export default () => (
	<Menubar.Root>
		<Menubar.Menu>
			<Menubar.Trigger>…</Menubar.Trigger>
			<Menubar.Portal>
				<Menubar.Content>
					<Menubar.Item className="MenubarItem" disabled>
						…
					</Menubar.Item>
					<Menubar.Item className="MenubarItem">…</Menubar.Item>
				</Menubar.Content>
			</Menubar.Portal>
		</Menubar.Menu>
	</Menubar.Root>
);
/* styles.css */
.MenubarItem[data-disabled] {
	color: gainsboro;
}
With separators
Use the Separator part to add a separator between items.

<Menubar.Root>
	<Menubar.Menu>
		<Menubar.Trigger>…</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content>
				<Menubar.Item>…</Menubar.Item>
				<Menubar.Separator />
				<Menubar.Item>…</Menubar.Item>
				<Menubar.Separator />
				<Menubar.Item>…</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>
</Menubar.Root>
With labels
Use the Label part to help label a section.

<Menubar.Root>
	<Menubar.Menu>
		<Menubar.Trigger>…</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content>
				<Menubar.Label>Label</Menubar.Label>
				<Menubar.Item>…</Menubar.Item>
				<Menubar.Item>…</Menubar.Item>
				<Menubar.Item>…</Menubar.Item>
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Menu>
</Menubar.Root>
With checkbox items
Use the CheckboxItem part to add an item that can be checked.

import * as React from "react";
import { CheckIcon } from "@radix-ui/react-icons";
import { Menubar } from "radix-ui";

export default () => {
	const [checked, setChecked] = React.useState(true);

	return (
		<Menubar.Root>
			<Menubar.Menu>
				<Menubar.Trigger>…</Menubar.Trigger>
				<Menubar.Portal>
					<Menubar.Content>
						<Menubar.Item>…</Menubar.Item>
						<Menubar.Item>…</Menubar.Item>
						<Menubar.Separator />
						<Menubar.CheckboxItem
							checked={checked}
							onCheckedChange={setChecked}
						>
							<Menubar.ItemIndicator>
								<CheckIcon />
							</Menubar.ItemIndicator>
							Checkbox item
						</Menubar.CheckboxItem>
					</Menubar.Content>
				</Menubar.Portal>
			</Menubar.Menu>
		</Menubar.Root>
	);
};
With radio items
Use the RadioGroup and RadioItem parts to add an item that can be checked amongst others.

import * as React from "react";
import { CheckIcon } from "@radix-ui/react-icons";
import { Menubar } from "radix-ui";

export default () => {
	const [color, setColor] = React.useState("blue");

	return (
		<Menubar.Root>
			<Menubar.Menu>
				<Menubar.Trigger>…</Menubar.Trigger>
				<Menubar.Portal>
					<Menubar.Content>
						<Menubar.RadioGroup value={color} onValueChange={setColor}>
							<Menubar.RadioItem value="red">
								<Menubar.ItemIndicator>
									<CheckIcon />
								</Menubar.ItemIndicator>
								Red
							</Menubar.RadioItem>
							<Menubar.RadioItem value="blue">
								<Menubar.ItemIndicator>
									<CheckIcon />
								</Menubar.ItemIndicator>
								Blue
							</Menubar.RadioItem>
						</Menubar.RadioGroup>
					</Menubar.Content>
				</Menubar.Portal>
			</Menubar.Menu>
		</Menubar.Root>
	);
};
With complex items
You can add extra decorative elements in the Item parts, such as images.

import { Menubar } from "radix-ui";

export default () => (
	<Menubar.Root>
		<Menubar.Menu>
			<Menubar.Trigger>…</Menubar.Trigger>
			<Menubar.Portal>
				<Menubar.Content>
					<Menubar.Item>
						<img src="…" />
						Adolfo Hess
					</Menubar.Item>
					<Menubar.Item>
						<img src="…" />
						Miyah Myles
					</Menubar.Item>
				</Menubar.Content>
			</Menubar.Portal>
		</Menubar.Menu>
	</Menubar.Root>
);
Constrain the content/sub-content size
You may want to constrain the width of the content (or sub-content) so that it matches the trigger
(or sub-trigger) width. You may also want to constrain its height to not exceed the viewport.

We expose several CSS custom properties such as --radix-menubar-trigger-width and
--radix-menubar-content-available-height to support this. Use them to constrain the content dimensions.

// index.jsx
import { Menubar } from "radix-ui";
import "./styles.css";

export default () => (
	<Menubar.Root>
		<Menubar.Trigger>…</Menubar.Trigger>
		<Menubar.Portal>
			<Menubar.Content className="MenubarContent" sideOffset={5}>
				…
			</Menubar.Content>
		</Menubar.Portal>
	</Menubar.Root>
);
/* styles.css */
.MenubarContent {
	width: var(--radix-menubar-trigger-width);
	max-height: var(--radix-menubar-content-available-height);
}

Origin-aware animations

We expose a CSS custom property --radix-menubar-content-transform-origin.
Use it to animate the content from its computed origin based on side, sideOffset, align, alignOffset and any collisions.

// index.jsx
import { Menubar } from "radix-ui";
import "./styles.css";

export default () => (
	<Menubar.Root>
		<Menubar.Menu>
			<Menubar.Trigger>…</Menubar.Trigger>
			<Menubar.Portal>
				<Menubar.Content className="MenubarContent">…</Menubar.Content>
			</Menubar.Portal>
		</Menubar.Menu>
	</Menubar.Root>
);
/* styles.css */
.MenubarContent {
	transform-origin: var(--radix-menubar-content-transform-origin);
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
import { Menubar } from "radix-ui";
import "./styles.css";

export default () => (
	<Menubar.Root>
		<Menubar.Menu>
			<Menubar.Trigger>…</Menubar.Trigger>
			<Menubar.Portal>
				<Menubar.Content className="MenubarContent">…</Menubar.Content>
			</Menubar.Portal>
		</Menubar.Menu>
	</Menubar.Root>
);
/* styles.css */
.MenubarContent {
	animation-duration: 0.6s;
	animation-timing-function: cubic-bezier(0.16, 1, 0.3, 1);
}
.MenubarContent[data-side="top"] {
	animation-name: slideUp;
}
.MenubarContent[data-side="bottom"] {
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



Keyboard Interactions
Key	                  Description
Space                   When focus is on MenubarTrigger, opens the menubar and focuses the first item.
                        When focus is on an item, activates the focused item.
Enter                   When focus is on MenubarTrigger, opens the associated menu.
                        When focus is on an item, activates the focused item.
ArrowDown               When focus is on MenubarTrigger, opens the associated menu.
                        When focus is on an item, moves focus to the next item.
ArrowUp                 When focus is on an item, moves focus to the previous item.
ArrowRight | ArrowLeft  When focus is on a MenubarTrigger, moves focus to the next or previous item.
                        When focus is on a MenubarSubTrigger, opens or closes the submenu depending on reading direction.
                        When focus is within a MenubarContent, opens the next menu in the menubar.
Esc                     Closes the currently open menu and moves focus to its MenubarTrigger.
*/

/**
 * @see MenubarPrimitiveRoot
 */
external interface MenubarRootProps : RadixProps, PropsWithAsChild {
    /**
     * The value of the menu that should be open when initially rendered.
     * Use when you do not need to control the value state.
     */
    var defaultValue: String?

    /**
     * The controlled value of the menu to open. Should be used in conjunction with onValueChange.
     */
    var value: String?

    /**
     * Event handler called when the value changes.
     */
    var onValueChange: ((value: String) -> Unit)?

    /**
     * The reading direction. If omitted, inherits globally from
     * DirectionProvider or assumes LTR (left-to-right) reading mode.
     */
    var dir: String? // "ltr" | "rtl"

    /**
     * When true, keyboard navigation will loop from last item to first, and vice versa.
     */
    var loop: Boolean?
}

/**
 * Contains all the parts of a menubar.
 */
@JsName("Root")
external val MenubarPrimitiveRoot: ComponentType<MenubarRootProps>

/**
 * @see MenubarPrimitiveMenu
 */
external interface MenubarMenuProps : RadixProps, PropsWithAsChild {
    /**
     * A unique value that associates the item with an active value when the navigation
     * menu is controlled. This prop is managed automatically when uncontrolled.
     */
    var value: String?
}

/**
 * A top level menu item, contains a trigger with content combination.
 */
@JsName("Menu")
external val MenubarPrimitiveMenu: ComponentType<MenubarMenuProps>

/**
 * @see MenubarPrimitiveTrigger
 */
external interface MenubarTriggerProps : RadixProps, PropsWithAsChild {
    @JsName("data-state")
    var dataState: String? // "open" | "closed"
    @JsName("data-highlighted")
    var dataHighlighted: Boolean?
    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * The button that toggles the content. By default, the MenubarContent
 * will position itself against the trigger.
 */
@JsName("Trigger")
external val MenubarPrimitiveTrigger: ComponentType<MenubarTriggerProps>

/**
 * @see MenubarPrimitivePortal
 */
external interface MenubarPortalProps : RadixProps {
    /**
     * Used to force mounting when more control is needed. Useful when controlling
     * animation with React animation libraries. If used on this part, it will be
     * inherited by MenubarContent and MenubarSubContent respectively.
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
external val MenubarPrimitivePortal: ComponentType<MenubarPortalProps>

/**
 * @see MenubarPrimitiveContent
 */
external interface MenubarContentProps : RadixProps, PropsWithAsChild {
    /**
     * When true, keyboard navigation will loop from last item to first, and vice versa.
     */
    var loop: Boolean?

    /**
     * Event handler called when focus moves to the trigger after closing.
     * It can be prevented by calling event.preventDefault.
     */
    var onCloseAutoFocus: ((Event) -> Unit)?

    /**
     * Event handler called when the escape key is down. It can be prevented by calling event.preventDefault
     */
    var onEscapeKeyDown: ((KeyboardEvent) -> Unit)?

    /**
     * Event handler called when a pointer event occurs outside the bounds of the component.
     * It can be prevented by calling event.preventDefault.
     */
    var onPointerDownOutside: ((Event) -> Unit)?

    /**
     * Event handler called when focus moves outside the bounds of the component.
     * It can be prevented by calling event.preventDefault.
     */
    var onFocusOutside: ((Event) -> Unit)?

    /**
     * Event handler called when an interaction (pointer or focus event) happens outside
     * the bounds of the component. It can be prevented by calling event.preventDefault.
     */
    var onInteractOutside: ((Event) -> Unit)?

    /**
     * Used to force mounting when more control is needed. Useful when controlling animation
     * with React animation libraries. It inherits from MenubarPortal.
     */
    var forceMount: Boolean?

    /**
     * The preferred side of the trigger to render against when open. Will be reversed when
     * collisions occur and avoidCollisions is enabled.
     */
    var side: String?

    /**
     * The distance in pixels from the trigger.
     */
    var sideOffset: Int?

    /**
     * The preferred alignment against the trigger. May change when collisions occur.
     */
    var align: String?

    /**
     * An offset in pixels from the "start" or "end" alignment options.
     */
    var alignOffset: Int?

    /**
     * When true, overrides the side and align preferences to prevent collisions with boundary edges.
     */
    var avoidCollisions: Boolean?

    /**
     * The element used as the collision boundary. By default, this is the viewport,
     * though you can provide additional element(s) to be included in this check.
     *
     * Element | null | Array<Element | null>
     */
    var collisionBoundary: dynamic // Element | null | Array<Element | null>

    /**
     * The distance in pixels from the boundary edges where collision detection should occur.
     * Accepts a number (same for all sides), or a partial padding object, for example:
     *
     * { top: 20, left: 20, bottom: 20, right: 20 }.
     *
     * unsafeJso {
     *    top = 20
     *    left = 20
     *    bottom = 20
     *    right = 20
     * }
     *
     * number | Padding
     */
    var collisionPadding: dynamic // number | Padding

    /**
     * The padding between the arrow and the edges of the content. If your content
     * has border-radius, this will prevent it from overflowing the corners.
     */
    var arrowPadding: Int?

    /**
     * The sticky behavior on the align axis. "partial" will keep the content in the
     * boundary as long as the trigger is at least partially in the boundary whilst
     * "always" will keep the content in the boundary regardless.
     *
     * "partial" | "always"
     */
    var sticky: String? // "partial" | "always"

    /**
     * Whether to hide the content when the trigger becomes fully occluded.
     */
    var hideWhenDetached: Boolean?

    @JsName("data-state")
    var dataState: String? // "open" | "closed"
    @JsName("data-side")
    var dataSide: String? // "left" | "right" | "bottom" | "top"
    @JsName("data-align")
    var dataAlign: String? // "start" | "end" | "center"
}

/*
CSS Variable	                           Description
--radix-menubar-content-transform-origin	The transform-origin computed from the content and arrow positions/offsets
--radix-menubar-content-available-width	The remaining width between the trigger and the boundary edge
--radix-menubar-content-available-height	The remaining height between the trigger and the boundary edge
--radix-menubar-trigger-width	            The width of the trigger
--radix-menubar-trigger-height            The height of the trigger
 */

/**
 * The component that pops out when a menu is open.
 */
@JsName("Content")
external val MenubarPrimitiveContent: ComponentType<MenubarContentProps>

/**
 * @see MenubarPrimitiveArrow
 */
external interface MenubarArrowProps : RadixProps, PropsWithAsChild {
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
 * An optional arrow element to render alongside a menubar menu. This can be used to help
 * visually link the trigger with the MenubarContent. Must be rendered inside MenubarContent.
 */
@JsName("Arrow")
external val MenubarPrimitiveArrow: ComponentType<MenubarArrowProps>

/**
 * @see MenubarPrimitiveItem
 */
external interface MenubarItemProps : RadixProps, PropsWithAsChild {
    /**
     * When true, prevents the user from interacting with the item.
     */
    var disabled: Boolean?

    /**
     * Event handler called when the user selects an item (via mouse or keyboard).
     * Calling event.preventDefault in this handler will prevent the menubar from
     * closing when selecting that item.
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
 * The component that contains the menubar items.
 */
@JsName("Item")
external val MenubarPrimitiveItem: ComponentType<MenubarItemProps>

/**
 * @see MenubarPrimitiveGroup
 */
external interface MenubarGroupProps : RadixProps, PropsWithAsChild

/**
 * Used to group multiple MenubarItems.
 */
@JsName("Group")
external val MenubarPrimitiveGroup: ComponentType<MenubarGroupProps>

/**
 * @see MenubarPrimitiveLabel
 */
external interface MenubarLabelProps : RadixProps, PropsWithAsChild

/**
 * Used to render a label. It won't be focusable using arrow keys.
 */
@JsName("Label")
external val MenubarPrimitiveLabel: ComponentType<MenubarLabelProps>

/**
 * @see MenubarPrimitiveCheckboxItem
 */
external interface MenubarCheckboxItemProps : RadixProps, PropsWithAsChild {
    /**
     * The controlled checked state of the item. Must be used in conjunction with onCheckedChange.
     */
    var checked: dynamic // boolean | "indeterminate"

    /**
     * Event handler called when the checked state changes.
     */
    var onCheckedChange: ((dynamic) -> Unit)? // boolean | "indeterminate"

    /**
     * When true, prevents the user from interacting with the item.
     */
    var disabled: Boolean?

    /**
     * Event handler called when the user selects an item (via mouse or keyboard).
     * Calling event.preventDefault in this handler will prevent the menubar from
     * closing when selecting that item.
     */
    var onSelect: ((Event) -> Unit)?

    /**
     * Optional text used for typeahead purposes. By default, the typeahead behavior will
     * use the .textContent of the item. Use this when the content is complex, or you
     * have non-textual content inside.
     */
    var textValue: String?

    @JsName("data-state")
    var dataState: String? // "checked" | "unchecked"
    @JsName("data-highlighted")
    var dataHighlighted: Boolean?
    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * An item that can be controlled and rendered like a checkbox.
 */
@JsName("CheckboxItem")
external val MenubarPrimitiveCheckboxItem: ComponentType<MenubarCheckboxItemProps>

/**
 * @see MenubarPrimitiveRadioGroup
 */
external interface MenubarRadioGroupProps : RadixProps, PropsWithAsChild {
    /**
     * The value of the selected item in the group.
     */
    var value: String?

    /**
     * Event handler called when the value changes.
     */
    var onValueChange: ((value: String) -> Unit)?
}

/**
 * Used to group multiple Menubar.RadioItems.
 */
@JsName("RadioGroup")
external val MenubarPrimitiveRadioGroup: ComponentType<MenubarRadioGroupProps>

/**
 * @see MenubarPrimitiveRadioItem
 */
external interface MenubarRadioItemProps : RadixProps, PropsWithAsChild {
    /**
     * The unique value of the item.
     */
    var value: String?

    /**
     * When true, prevents the user from interacting with the item.
     */
    var disabled: Boolean?

    /**
     * Event handler called when the user selects an item (via mouse or keyboard).
     * Calling event.preventDefault in this handler will prevent the menubar from
     * closing when selecting that item.
     */
    var onSelect: ((Event) -> Unit)?

    /**
     * Optional text used for typeahead purposes. By default, the typeahead behavior
     * will use the .textContent of the item. Use this when the content is complex,
     * or you have non-textual content inside.
     */
    var textValue: String?

    @JsName("data-state")
    var dataState: String? // "checked" | "unchecked"
    @JsName("data-highlighted")
    var dataHighlighted: Boolean?
    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * An item that can be controlled and rendered like a radio.
 */
@JsName("RadioItem")
external val MenubarPrimitiveRadioItem: ComponentType<MenubarRadioItemProps>

/**
 * @see MenubarPrimitiveItemIndicator
 */
external interface MenubarItemIndicatorProps : RadixProps, PropsWithAsChild {
    /**
     * Used to force mounting when more control is needed. Useful when controlling
     * animation with React animation libraries.
     */
    var forceMount: Boolean?

    @JsName("data-state")
    var dataState: String? // "checked" | "unchecked"
}

/**
 * Renders when the parent MenubarCheckboxItem or MenubarRadioItem is checked.
 * You can style this element directly, or you can use it as a wrapper to put an
 * icon into, or both.
 */
@JsName("ItemIndicator")
external val MenubarPrimitiveItemIndicator: ComponentType<MenubarItemIndicatorProps>

/**
 * @see MenubarPrimitiveSeparator
 */
external interface MenubarSeparatorProps : RadixProps, PropsWithAsChild

/**
 * Used to visually separate items in a menubar menu.
 */
@JsName("Separator")
external val MenubarPrimitiveSeparator: ComponentType<MenubarSeparatorProps>

external interface MenubarSubProps : RadixProps {
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
external val MenubarPrimitiveSub: ComponentType<MenubarSubProps>

/**
 * @see MenubarPrimitiveSubTrigger
 */
external interface MenubarSubTriggerProps : RadixProps, PropsWithAsChild {
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
    var dataState: String? // "checked" | "unchecked"
    @JsName("data-highlighted")
    var dataHighlighted: Boolean?
    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * An item that opens a submenu. Must be rendered inside MenubarSub.
 */
@JsName("SubTrigger")
external val MenubarPrimitiveSubTrigger: ComponentType<MenubarSubTriggerProps>

/**
 * @see MenubarPrimitiveSubContent
 */
external interface MenubarSubContentProps : RadixProps, PropsWithAsChild {
    /**
     * When true, keyboard navigation will loop from last item to first, and vice versa.
     */
    var loop: Boolean?

    /**
     * Event handler called when the escape key is down. It can be prevented by calling event.preventDefault
     */
    var onEscapeKeyDown: ((KeyboardEvent) -> Unit)?

    /**
     * Event handler called when a pointer event occurs outside the bounds of the component.
     * It can be prevented by calling event.preventDefault.
     */
    var onPointerDownOutside: ((Event) -> Unit)?

    /**
     * Event handler called when focus moves outside the bounds of the component.
     * It can be prevented by calling event.preventDefault.
     */
    var onFocusOutside: ((Event) -> Unit)?

    /**
     * Event handler called when an interaction (pointer or focus event) happens outside
     * the bounds of the component. It can be prevented by calling event.preventDefault.
     */
    var onInteractOutside: ((Event) -> Unit)?

    /**
     * Used to force mounting when more control is needed. Useful when controlling animation
     * with React animation libraries. It inherits from MenubarPortal.
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
     * When true, overrides the side and align preferences to prevent collisions with boundary edges.
     */
    var avoidCollisions: Boolean?

    /**
     * The element used as the collision boundary. By default, this is the viewport,
     * though you can provide additional element(s) to be included in this check.
     *
     * Element | null | Array<Element | null>
     */
    var collisionBoundary: dynamic // Element | null | Array<Element | null>

    /**
     * The distance in pixels from the boundary edges where collision detection should occur.
     * Accepts a number (same for all sides), or a partial padding object, for example:
     *
     * { top: 20, left: 20, bottom: 20, right: 20 }.
     *
     * unsafeJso {
     *    top = 20
     *    left = 20
     *    bottom = 20
     *    right = 20
     * }
     *
     * number | Padding
     */
    var collisionPadding: dynamic // number | Padding

    /**
     * The padding between the arrow and the edges of the content. If your content
     * has border-radius, this will prevent it from overflowing the corners.
     */
    var arrowPadding: Int?

    /**
     * The sticky behavior on the align axis. "partial" will keep the content in the
     * boundary as long as the trigger is at least partially in the boundary whilst
     * "always" will keep the content in the boundary regardless.
     *
     * "partial" | "always"
     */
    var sticky: String? // "partial" | "always"

    /**
     * Whether to hide the content when the trigger becomes fully occluded.
     */
    var hideWhenDetached: Boolean?

    @JsName("data-state")
    var dataState: String? // "open" | "closed"
    @JsName("data-side")
    var dataSide: String? // "left" | "right" | "bottom" | "top"
    @JsName("data-align")
    var dataAlign: String? // "start" | "end" | "center"
    @JsName("data-orientation")
    var dataOrientation: String? // "vertical" | "horizontal"
}

/*
CSS Variable	                           Description
--radix-menubar-content-transform-origin	The transform-origin computed from the content and arrow positions/offsets
--radix-menubar-content-available-width	The remaining width between the trigger and the boundary edge
--radix-menubar-content-available-height	The remaining height between the trigger and the boundary edge
--radix-menubar-trigger-width	            The width of the trigger
--radix-menubar-trigger-height            The height of the trigger
*/

/**
 * The component that pops out when a submenu is open. Must be rendered inside MenubarSub.
 */
@JsName("SubContent")
external val MenubarPrimitiveSubContent: ComponentType<MenubarSubContentProps>
