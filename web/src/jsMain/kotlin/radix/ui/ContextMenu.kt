@file:JsModule("@radix-ui/react-context-menu") @file:JsNonModule

package radix.ui

import react.ComponentType
import react.Props
import react.PropsWithChildren
import react.PropsWithClassName
import react.PropsWithStyle
import react.PropsWithValue
import web.html.HTMLElement

@JsName("Root")
external val ContextMenuRoot: ComponentType<ContextMenuRootProps>

external interface ContextMenuRootProps : PropsWithChildren {
//   var open: Boolean?
//   var defaultOpen: Boolean?
   var onOpenChange: ((Boolean) -> Unit)?
   var modal: Boolean?
   var dir: String? /* "ltr" | "rtl" */
}

@JsName("Trigger")
external val ContextMenuTrigger: ComponentType<ContextMenuTriggerProps>

external interface ContextMenuTriggerProps : PropsWithChildren {
   var asChild: Boolean?
   var disabled: Boolean?
   @JsName("data-state")
   var dataState: String? // "open" | "closed"
}

@JsName("Portal")
external val ContextMenuPortal: ComponentType<ContextMenuPortalProps>

external interface ContextMenuPortalProps : PropsWithChildren {
   var forceMount: Boolean?
   var container: HTMLElement?
}

@JsName("Content")
external val ContextMenuContent: ComponentType<ContextMenuContentProps>

external interface ContextMenuContentProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var asChild: Boolean?
   var loop: Boolean?
   var onCloseAutoFocus: ((dynamic) -> Unit)?
   var onEscapeKeyDown: ((dynamic) -> Unit)?
   var onPointerDownOutside: ((dynamic) -> Unit)?
   var onFocusOutside: ((dynamic) -> Unit)?
   var onInteractOutside: ((dynamic) -> Unit)?
   var onOpenAutoFocus: ((dynamic) -> Unit)?
   var forceMount: Boolean?
   var alignOffset: Int?
   var avoidCollisions: Boolean?
   var collisionBoundary: dynamic /* HTMLElement | Array<HTMLElement> */
   var collisionPadding: dynamic /* Int | { top: Int, right: Int, bottom: Int, left: Int } */
   var sticky: String? /* "partial" | "always" */
   var hideWhenDetached: Boolean?

   @JsName("data-state")
   var dataState: String // "open" | "closed"
   // data-side
   @JsName("data-side")
   var dataSide: String? /* "top" | "right" | "bottom" | "left" */
   @JsName("data-align")
   var dataAlign: String? /* "start" | "center" | "end" */

   var sideOffset: Int?
   var arrowPadding: Int?
}

@JsName("Arrow")
external val ContextMenuArrow: ComponentType<ContextMenuArrowProps>

external interface ContextMenuArrowProps : PropsWithClassName, PropsWithStyle {
   var asChild: Boolean?
   var width: Int?
   var height: Int?
}

@JsName("Item")
external val ContextMenuItem: ComponentType<ContextMenuItemProps>

external interface ContextMenuItemProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var asChild: Boolean?
   var disabled: Boolean?
   var onSelect: ((dynamic) -> Unit)?
   var textValue: String?
}

@JsName("Group")
external val ContextMenuGroup: ComponentType<ContextMenuGroupProps>

external interface ContextMenuGroupProps : PropsWithChildren {
   var asChild: Boolean?
}

@JsName("Label")
external val ContextMenuLabel: ComponentType<ContextMenuLabelProps>

external interface ContextMenuLabelProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var asChild: Boolean?
   var inset: dynamic?
}

@JsName("CheckboxItem")
external val ContextMenuCheckboxItem: ComponentType<ContextMenuCheckboxItemProps>

external interface ContextMenuCheckboxItemProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var asChild: Boolean?
   var checked: dynamic /* Boolean | "indeterminate" */
   var onCheckedChange: ((dynamic) -> Unit)?
   var disabled: Boolean?
   var onSelect: ((dynamic) -> Unit)?
   var textValue: String?
   @JsName("data-state")
   var dataState: String? // "checked" | "unchecked" | "indeterminate"
   @JsName("data-highlighted")
   var dataHighlighted: String?
   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

@JsName("RadioGroup")
external val ContextMenuRadioGroup: ComponentType<ContextMenuRadioGroupProps>

external interface ContextMenuRadioGroupProps : PropsWithChildren, PropsWithValue<String> {
   var asChild: Boolean?
   var onValueChange: ((String) -> Unit)?
}

@JsName("RadioItem")
external val ContextMenuRadioItem: ComponentType<ContextMenuRadioItemProps>

external interface ContextMenuRadioItemProps : PropsWithChildren, PropsWithClassName, PropsWithStyle, PropsWithValue<String> {
   var asChild: Boolean?
   var disabled: Boolean?
   var onSelect: ((dynamic) -> Unit)?
   var textValue: String?
   @JsName("data-state")
   var dataState: String? // "checked" | "unchecked" | "indeterminate"
   @JsName("data-highlighted")
   var dataHighlighted: dynamic
   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

@JsName("ItemIndicator")
external val ContextMenuItemIndicator: ComponentType<ContextMenuItemIndicatorProps>

external interface ContextMenuItemIndicatorProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var asChild: Boolean?
   var forceMount: Boolean?
   @JsName("data-state")
   var dataState: String? // "checked" | "unchecked" | "indeterminate"
}

@JsName("Separator")
external val ContextMenuSeparator: ComponentType<ContextMenuSeparatorProps>

external interface ContextMenuSeparatorProps : PropsWithClassName, PropsWithStyle {
   var asChild: Boolean?
}

@JsName("Sub")
external val ContextMenuSub: ComponentType<ContextMenuSubProps>

external interface ContextMenuSubProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var defaultOpen: Boolean?
   var open: Boolean?
   var onOpenChange: ((dynamic) -> Unit)?
}

@JsName("SubTrigger")
external val ContextMenuSubTrigger: ComponentType<ContextMenuSubTriggerProps>

external interface ContextMenuSubTriggerProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var disabled: Boolean?
   var textValue: String?
   var inset: String?
   @JsName("data-state")
   var dataState: String? // "checked" | "unchecked" | "indeterminate"
   @JsName("data-highlighted")
   var dataHighlighted: dynamic
   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

@JsName("SubContent")
external val ContextMenuSubContent: ComponentType<ContextMenuSubContentProps>

external interface ContextMenuSubContentProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var asChild: Boolean?
   var loop: Boolean?
   var onEscapeKeyDown: ((dynamic) -> Unit)?
   var onPointerDownOutside: ((dynamic) -> Unit)?
   var onFocusOutside: ((dynamic) -> Unit)?
   var onInteractOutside: ((dynamic) -> Unit)?
   var forceMount: Boolean?
   var sideOffset: Int?
   var alignOffset: Int?
   var avoidCollisions: Boolean?
   var collisionBoundary: dynamic /* HTMLElement | Array<HTMLElement> */
   var collisionPadding: dynamic /* Int | { top: Int, right: Int, bottom: Int, left: Int } */
   var arrowPadding: Int?
   var sticky: String? /* "partial" | "always" */
   var hideWhenDetached: Boolean?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"
   @JsName("data-side")
   var side: String? // "left" | "right" | "bottom" | "top"
   @JsName("data-align")
   var align: String? // "start" | "center" | "end"

   var onCloseAutoFocus: ((dynamic) -> Unit)?
   var onOpenAutoFocus: ((dynamic) -> Unit)?
}
