@file:JsModule("@radix-ui/react-dropdown-menu")
@file:JsNonModule

package radix.ui

import react.ComponentType
import react.PropsWithValue
import web.events.Event
import web.uievents.KeyboardEvent

/*

<DropdownMenu.Root>
	<DropdownMenu.Trigger>…</DropdownMenu.Trigger>
	<DropdownMenu.Portal>
		<DropdownMenu.Content>
			<DropdownMenu.Item>…</DropdownMenu.Item>
			<DropdownMenu.Item>…</DropdownMenu.Item>
			<DropdownMenu.Separator />
			<DropdownMenu.Sub>
				<DropdownMenu.SubTrigger>Sub menu →</DropdownMenu.SubTrigger>
				<DropdownMenu.Portal>
					<DropdownMenu.SubContent>
						<DropdownMenu.Item>Sub menu item</DropdownMenu.Item>
						<DropdownMenu.Item>Sub menu item</DropdownMenu.Item>
						<DropdownMenu.Arrow />
					</DropdownMenu.SubContent>
				</DropdownMenu.Portal>
			</DropdownMenu.Sub>
			<DropdownMenu.Separator />
			<DropdownMenu.Item>…</DropdownMenu.Item>
		</DropdownMenu.Content>
	</DropdownMenu.Portal>
</DropdownMenu.Root>

*/

/**
 * Contains all the parts of a dropdown menu.
 */
@JsName("Root")
external val DropdownMenuRoot: ComponentType<DefaultProps>

/**
 * @see DropdownMenuRoot
 */
external interface DropdownMenuRootProps : DefaultProps {
    /**
     * The open state of the dropdown menu when it is initially rendered.
     * Use when you do not need to control its open state.
     */
    var defaultOpen: Boolean?

    /**
     * The controlled open state of the dropdown menu.
     * Must be used in conjunction with onOpenChange.
     */
    var open: Boolean?

    /**
     * Event handler called when the open state of the dropdown menu changes.
     */
    var onOpenChange: ((open: Boolean) -> Unit)?

    /**
     * The modality of the dropdown menu. When set to true, interaction with outside
     * elements will be disabled and only menu content will be visible to screen readers.
     */
    var modal: Boolean?

    /**
     * The reading direction of submenus when applicable. If omitted, inherits
     * globally from DirectionProvider or assumes LTR (left-to-right) reading mode.
     */
    var dir: String? // "ltr" | "rtl"
}

/**
 * @see DropdownMenuTrigger
 */
external interface DropdownMenuTriggerProps : DefaultProps {
    var asChild: Boolean?

    @JsName("data-state")
    var dataState: String? // "open" | "closed"
    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * The button that toggles the dropdown menu. By default, the DropdownMenuContent will
 * position itself against the trigger.
 */
@JsName("Trigger")
external val DropdownMenuTrigger: ComponentType<DropdownMenuTriggerProps>

/**
 * @see DropdownMenuPortal
 */
external interface DropdownMenuPortalProps : DefaultProps {
    /**
     * Used to force mounting when more control is needed. Useful when controlling
     * animation with React animation libraries. If used on this part, it will be
     * inherited by DropdownMenu.Content and DropdownMenu.SubContent respectively.
     */
    var forceMount: Boolean?

    /**
     * Specify a container element to portal the content into.
     */
    var container: dynamic // HTMLElement or null
}

/**
 * When used, portals the content part into the body.
 */
@JsName("Portal")
external val DropdownMenuPortal: ComponentType<DropdownMenuPortalProps>

/**
 * @see DropdownMenuContent
 */
external interface DropdownMenuContentProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child,
     * merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?

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
     * Event handler called when the escape key is down. It can be prevented
     * by calling event.preventDefault
     */
    var onEscapeKeyDown: ((KeyboardEvent) -> Unit)?

    /**
     * Event handler called when a pointer event occurs outside the bounds
     * of the component. It can be prevented by calling event.preventDefault.
     */
    var onPointerDownOutside: ((Event) -> Unit)?

    /**
     * Event handler called when focus moves outside the bounds of the component.
     * It can be prevented by calling event.preventDefault.
     */
    var onFocusOutside: ((Event) -> Unit)?

    /**
     * Event handler called when an interaction (pointer or focus event) happens
     * outside the bounds of the component. It can be prevented by calling event.preventDefault.
     */
    var onInteractOutside: ((Event) -> Unit)?

    /**
     * Used to force mounting when more control is needed. Useful when controlling animation
     * with React animation libraries. It inherits from DropdownMenuPortal.
     */
    var forceMount: Boolean?

    /**
     * The preferred side of the trigger to render against when open. Will be reversed
     * when collisions occur and avoidCollisions is enabled.
     */
    var side: String? // "top" | "right" | "bottom" | "left"

    /**
     * The distance in pixels from the trigger.
     */
    var sideOffset: Number?

    /**
     * The preferred alignment against the trigger. May change when collisions occur.
     */
    var align: String? // "start" | "center" | "end"

    /**
     * An offset in pixels from the "start" or "end" alignment options.
     */
    var alignOffset: Number?

    /**
     * When true, overrides the side andalign preferences to prevent collisions with boundary edges.
     */
    var avoidCollisions: Boolean?

    /**
     * The element used as the collision boundary. By default, this is the viewport, though
     * you can provide additional element(s) to be included in this check.
     */
    var collisionBoundary: dynamic // Element | null | Array<Element | null>

    /**
     * The distance in pixels from the boundary edges where collision detection should occur.
     * Accepts a number (same for all sides), or a partial padding object,
     *
     * for example: { top: 20, left: 20 }.
     * unsafeJso {
     *    top = 20
     *    left = 20
     * }
     */
    var collisionPadding: dynamic // number | Partial<Record<Side, number>>

    /**
     * The padding between the arrow and the edges of the content. If your content has
     * border-radius, this will prevent it from overflowing the corners.
     */
    var arrowPadding: Number?

    /**
     * The sticky behavior on the align axis. "partial" will keep the content in the boundary
     * as long as the trigger is at least partially in the boundary whilst "always" will keep
     * the content in the boundary regardless.
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
    var dataAlign: String? // "start" | "end" | "center
    @JsName("data-orientation")
    var dataOrientation: String? // "vertical" | "horizontal"
}

/*
CSS Variable	                                 Description
--radix-dropdown-menu-content-transform-origin	The transform-origin computed from the content and arrow positions/offsets
--radix-dropdown-menu-content-available-width	The remaining width between the trigger and the boundary edge
--radix-dropdown-menu-content-available-height	The remaining height between the trigger and the boundary edge
--radix-dropdown-menu-trigger-width	            The width of the trigger
--radix-dropdown-menu-trigger-height
 */

/**
 * The component that pops out when the dropdown menu is open.
 */
@JsName("Content")
external val DropdownMenuContent: ComponentType<DropdownMenuContentProps>

/**
 * @see DropdownMenuArrow
 */
external interface DropdownMenuArrowProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?

    /**
     * The width of the arrow in pixels.
     */
    var width: Number?

    /**
     * The height of the arrow in pixels.
     */
    var height: Number?
}

/**
 * An optional arrow element to render alongside the dropdown menu. This can be used
 * to help visually link the trigger with the DropdownMenu.Content. Must be rendered
 * inside DropdownMenu.Content.
 */
@JsName("Arrow")
external val DropdownMenuArrow: ComponentType<DropdownMenuArrowProps>

/**
 * @see DropdownMenuItem
 */
external interface DropdownMenuItemProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?

    /**
     * When true, prevents the user from interacting with the item.
     */
    var disabled: Boolean?

    /**
     * Event handler called when the user selects an item (via mouse or keyboard).
     * Calling event.preventDefault in this handler will prevent the dropdown menu
     * from closing when selecting that item.
     */
    var onSelect: ((Event) -> Unit)?

    /**
     * Optional text used for typeahead purposes. By default, the typeahead behavior
     * will use the .textContent of the item. Use this when the content is complex,
     * or you have non-textual content inside.
     */
    var textValue: String?

    @JsName("data-orientation")
    var dataOrientation: String? // "vertical" | "horizontal"

    @JsName("data-highlighted")
    var dataHighlighted: Boolean? // Present when highlighted

    @JsName("data-disabled")
    var dataDisabled: Boolean? // Present when disabled
}

/**
 * The component that contains the dropdown menu items.
 */
@JsName("Item")
external val DropdownMenuItem: ComponentType<DropdownMenuItemProps>

/**
 * @see DropdownMenuGroup
 */
external interface DropdownMenuGroupProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?
}

/**
 * Used to group multiple DropdownMenuItems.
 */
@JsName("Group")
external val DropdownMenuGroup: ComponentType<DropdownMenuGroupProps>

/**
 * @see DropdownMenuLabel
 */
external interface DropdownMenuLabelProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?
}

@JsName("Label")
external val DropdownMenuLabel: ComponentType<DropdownMenuLabelProps>

/**
 * @see DropdownMenuCheckboxItem
 */
external interface DropdownMenuCheckboxItemProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?

    /**
     * The controlled checked state of the item. Must be used in conjunction with onCheckedChange.
     */
    var checked: dynamic // boolean | 'indeterminate'

    /**
     * Event handler called when the checked state changes.
     */
    var onCheckedChange: ((checked: Boolean) -> Unit)?

    /**
     * When true, prevents the user from interacting with the item.
     */
    var disabled: Boolean?

    /**
     * Event handler called when the user selects an item (via mouse or keyboard).
     * Calling event.preventDefault in this handler will prevent the dropdown menu
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
    var dataHighlighted: Boolean? // Present when highlighted

    @JsName("data-disabled")
    var dataDisabled: Boolean? // Present when disabled
}

/**
 * An item that can be controlled and rendered like a checkbox.
 */
@JsName("CheckboxItem")
external val DropdownMenuCheckboxItem: ComponentType<DropdownMenuCheckboxItemProps>

/**
 * @see DropdownMenuRadioGroup
 */
external interface DropdownMenuRadioGroupProps : DefaultProps, PropsWithValue<String> {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?

    /**
     * Event handler called when the value changes.
     */
    var onValueChange: ((String) -> Unit)?
}

/**
 * Used to group multiple DropdownMenuRadioItems.
 */
@JsName("RadioGroup")
external val DropdownMenuRadioGroup: ComponentType<DropdownMenuRadioGroupProps>

/**
 * @see DropdownMenuRadioItem
 */
external interface DropdownMenuRadioItemProps : DefaultProps, PropsWithValue<String> {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?

    /**
     * When true, prevents the user from interacting with the item.
     */
    var disabled: Boolean?

    /**
     * Event handler called when the user selects an item (via mouse or keyboard).
     * Calling event.preventDefault in this handler will prevent the dropdown menu
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
    var dataHighlighted: Boolean? // Present when highlighted

    @JsName("data-disabled")
    var dataDisabled: Boolean? // Present when disabled
}

/**
 * An item that can be controlled and rendered like a radio.
 */
@JsName("RadioItem")
external val DropdownMenuRadioItem: ComponentType<DropdownMenuRadioItemProps>

/**
 * @see DropdownMenuSeparator
 */
external interface DropdownMenuItemIndicatorProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?

    /**
     * Used to force mounting when more control is needed. Useful when controlling
     * animation with React animation libraries.
     */
    var forceMount: Boolean?
}

/**
 * Renders when the parent DropdownMenuCheckboxItem or DropdownMenuRadioItem is checked.
 * You can style this element directly, or you can use it as a wrapper to put an icon into, or both.
 */
@JsName("ItemIndicator")
external val DropdownMenuItemIndicator: ComponentType<DropdownMenuItemIndicatorProps>

/**
 * @see DropdownMenuSeparator
 */
external interface DropdownMenuSeparatorProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?
}

/**
 * Used to visually separate items in the dropdown menu.
 */
@JsName("Separator")
external val DropdownMenuSeparator: ComponentType<DropdownMenuSeparatorProps>

/**
 * @see DropdownMenuSub
 */
external interface DropdownMenuSubProps : DefaultProps {
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
external val DropdownMenuSub: ComponentType<DropdownMenuSubProps>

/**
 * @see DropdownMenuSubTrigger
 */
external interface DropdownMenuSubTriggerProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child, merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?

    /**
     * When true, prevents the user from interacting with the item.
     */
    var disabled: Boolean?

    /**
     * Optional text used for typeahead purposes. By default, the typeahead behavior will
     * use the .textContent of the item. Use this when the content is complex, or you
     * have non-textual content inside.
     */
    var textValue: String?

    @JsName("data-state")
    var dataState: String? // "open" | "closed"

    @JsName("data-highlighted")
    var dataHighlighted: Boolean? // Present when highlighted

    @JsName("data-disabled")
    var dataDisabled: Boolean? // Present when disabled
}

/*
CSS Variable	Description
--radix-dropdown-menu-content-transform-origin	The transform-origin computed from the content and arrow positions/offsets
--radix-dropdown-menu-content-available-width	The remaining width between the trigger and the boundary edge
--radix-dropdown-menu-content-available-height	The remaining height between the trigger and the boundary edge
--radix-dropdown-menu-trigger-width	            The width of the trigger
--radix-dropdown-menu-trigger-height            The height of the trigger
 */

@JsName("SubTrigger")
external val DropdownMenuSubTrigger: ComponentType<DropdownMenuSubTriggerProps>

/**
 * @see DropdownMenuSubContent
 */
external interface DropdownMenuSubContentProps : DefaultProps {
    /**
     * Change the default rendered element for the one passed as a child,
     * merging their props and behavior.
     *
     * Read our Composition guide for more details.
     */
    var asChild: Boolean?

    /**
     * When true, keyboard navigation will loop from last item to first, and vice versa.
     */
    var loop: Boolean?

    /**
     * Event handler called when the escape key is down. It can be prevented
     * by calling event.preventDefault
     */
    var onEscapeKeyDown: ((KeyboardEvent) -> Unit)?

    /**
     * Event handler called when a pointer event occurs outside the bounds
     * of the component. It can be prevented by calling event.preventDefault.
     */
    var onPointerDownOutside: ((Event) -> Unit)?

    /**
     * Event handler called when focus moves outside the bounds of the component.
     * It can be prevented by calling event.preventDefault.
     */
    var onFocusOutside: ((Event) -> Unit)?

    /**
     * Event handler called when an interaction (pointer or focus event) happens
     * outside the bounds of the component. It can be prevented by calling event.preventDefault.
     */
    var onInteractOutside: ((Event) -> Unit)?

    /**
     * Used to force mounting when more control is needed. Useful when controlling animation
     * with React animation libraries. It inherits from DropdownMenuPortal.
     */
    var forceMount: Boolean?

    /**
     * The distance in pixels from the trigger.
     */
    var sideOffset: Number?

    /**
     * An offset in pixels from the "start" or "end" alignment options.
     */
    var alignOffset: Number?

    /**
     * When true, overrides the side andalign preferences to prevent collisions with boundary edges.
     */
    var avoidCollisions: Boolean?

    /**
     * The element used as the collision boundary. By default, this is the viewport, though
     * you can provide additional element(s) to be included in this check.
     */
    var collisionBoundary: dynamic // Element | null | Array<Element | null>

    /**
     * The distance in pixels from the boundary edges where collision detection should occur.
     * Accepts a number (same for all sides), or a partial padding object,
     *
     * for example: { top: 20, left: 20 }.
     * unsafeJso {
     *    top = 20
     *    left = 20
     * }
     */
    var collisionPadding: dynamic // number | Partial<Record<Side, number>>

    /**
     * The padding between the arrow and the edges of the content. If your content has
     * border-radius, this will prevent it from overflowing the corners.
     */
    var arrowPadding: Number?

    /**
     * The sticky behavior on the align axis. "partial" will keep the content in the boundary
     * as long as the trigger is at least partially in the boundary whilst "always" will keep
     * the content in the boundary regardless.
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
    var dataAlign: String? // "start" | "end" | "center
    @JsName("data-orientation")
    var dataOrientation: String? // "vertical" | "horizontal"
}

/**
 * The component that pops out when a submenu is open. Must be rendered inside DropdownMenu.Sub.
 */
@JsName("SubContent")
external val DropdownMenuSubContent: ComponentType<DropdownMenuSubContentProps>



