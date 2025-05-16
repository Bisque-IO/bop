@file:JsModule("vaul")
@file:JsNonModule

package vaul

import react.*
import react.dom.events.ChangeEvent
import web.html.HTMLElement
import web.html.HTMLInputElement
import web.html.HTMLTextAreaElement

@JsName("Drawer")
external object Drawer {
   val Root: ComponentType<DrawerRootProps>
   val Trigger: ComponentType<DrawerTriggerProps>
   val Portal: ComponentType<PropsWithChildren>
   val Overlay: ComponentType<DrawerOverlayProps>
   val Content: ComponentType<DrawerContentProps>
   val Title: ComponentType<DrawerTitleProps>
   val Description: ComponentType<DrawerDescriptionProps>
   val Close: ComponentType<DrawerCloseProps>
   val Input: ComponentType<DrawerInputProps>
   val Textarea: ComponentType<DrawerTextareaProps>
   val Handle: ComponentType<PropsWithChildren>
}

external interface DrawerRootProps : PropsWithChildren, PropsWithClassName, PropsWithStyle, PropsWithRef<HTMLElement> {
   /**
    * The open state of the drawer when it is initially rendered.
    * Use when you do not need to control its open state.
    */
   var defaultOpen: Boolean?

   /**
    * The controlled open state of the drawer.
    */
   var open: Boolean?

   /**
    * Event handler called when the open state of the drawer changes.
    */
   var onOpenChange: ((open: Boolean) -> Unit)?

   /**
    * The modality of the dialog. When set to true, interaction with outside
    * elements will be disabled and only dialog content will be visible to
    * screen readers.
    */
   var modal: Boolean?

   /**
    * Specify a container element to portal the drawer into.
    */
   var container: HTMLElement?

   /**
    * Direction of the drawer. This adjust the animations and the drag direction.
    */
   var direction: String? // "top" | "right" | "bottom" | "left"

   /**
    * Gets triggered after the open or close animation ends, it receives an open
    * argument with the open state of the drawer by the time the function was triggered.
    */
   var onAnimationEnd: ((open: Boolean) -> Unit)?

   /**
    * When false dragging, clicking outside, pressing esc, etc. will not close the drawer.
    * Use this in combination with the open prop, otherwise you won't be able to open/close
    * the drawer.
    */
   var dismissible: Boolean?

   /**
    * When true dragging will only be possible by the handle.
    */
   var handleOnly: Boolean?

   /**
    * When true Vaul will reposition inputs rather than scroll then into view if the keyboard
    * is in the way. Setting it to false will fall back to the default browser behavior.
    */
   var repositionInputs: Boolean?

   /**
    * Array of numbers from 0 to 1 that corresponds to % of the screen a given snap point should take up.
    * You can also use px values, which doesn't take screen height into account.
    */
   var snapPoints: Array<Number>?

   /**
    * The controlled snap point state.
    */
   var activeSnapPoint: Boolean?

   /**
    * Event handler called when the snap point state changes.
    */
   var setActiveSnapPoint: ((open: Boolean) -> Unit)?

   /**
    * Index of a snapPoint from which the overlay fade should be applied. Defaults
    * to the last snap point.
    */
   var fadeFromIndex: Number?

   /**
    * Disabled velocity based swiping for snap points. This means that a snap point
    * won't be skipped even if the velocity is high enough. Useful if each snap point
    * in a drawer is equally important.
    */
   var snapToSequentialPoint: Boolean?
}

external interface DrawerTriggerProps : PropsWithChildren, PropsWithClassName, PropsWithStyle, PropsWithRef<HTMLElement> {
   var asChild: Boolean?
}

external interface DrawerOverlayProps : PropsWithChildren, PropsWithClassName, PropsWithStyle, PropsWithRef<HTMLElement> {
   var asChild: Boolean?
}

external interface DrawerContentProps : PropsWithChildren, PropsWithClassName, PropsWithStyle, PropsWithRef<HTMLElement> {
   var asChild: Boolean?
}

external interface DrawerCloseProps : PropsWithChildren, PropsWithClassName, PropsWithStyle, PropsWithRef<HTMLElement> {
   var asChild: Boolean?
}

external interface DrawerTitleProps : PropsWithChildren, PropsWithClassName, PropsWithStyle, PropsWithRef<HTMLElement> {
   var asChild: Boolean?
}

external interface DrawerDescriptionProps : PropsWithChildren, PropsWithClassName, PropsWithStyle, PropsWithRef<HTMLElement> {
   var asChild: Boolean?
}

external interface DrawerInputProps : Props {
   var value: String?
   var onChange: ((ChangeEvent<HTMLInputElement>) -> Unit)?
   var placeholder: String?
   var className: String?
   var style: dynamic
   var ref: Ref<HTMLInputElement>?
}

external interface DrawerTextareaProps : Props {
   var value: String?
   var onChange: ((ChangeEvent<HTMLTextAreaElement>) -> Unit)?
   var placeholder: String?
   var className: String?
   var style: dynamic
   var ref: Ref<HTMLTextAreaElement>?
}