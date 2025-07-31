@file:JsModule("sonner") @file:JsNonModule

package lib.sonner

import react.*

external interface ToastOptions {
   /**
    * Toast's description, renders underneath the title.
    */
   var description: ReactNode?

   /**
    * Adds a close button.
    */
   var closeButton: Boolean?

   /**
    * Dark toast in light mode and vice versa.
    */
   var invert: Boolean?

   /**
    * Time in milliseconds that should elapse before automatically closing the toast.
    */
   var duration: Int?

   /**
    * Position of the toast.
    */
   var position: String?

   /**
    * If 'false', it'll prevent the user from dismissing the toast.
    */
   var dismissible: Boolean?

   /**
    * Icon displayed in front of toast's text, aligned vertically.
    */
   var icon: ReactNode?

   /**
    * Renders a primary button, clicking it will close the toast.
    */
   var action: ReactNode?

   /**
    * Renders a secondary button, clicking it will close the toast.
    */
   var cancel: ReactNode?

   /**
    * Custom id for the toast.
    */
   var id: String?

   /**
    * The function gets called when either the close button is clicked, or the toast is swiped.
    */
   var onDismiss: (() -> Unit)?

   /**
    * Function that gets called when the toast disappears automatically after its timeout (duration prop).
    */
   var onAutoClose: (() -> Unit)?

   /**
    * Styles for the action button.
    */
   var actionButtonStyle: dynamic

   /**
    * Styles for the cancel button.
    */
   var cancelButtonStyle: dynamic
}

external interface ToasterProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   /**
    * Toast's theme, either 'light', 'dark', or 'system'.
    */
   var theme: String? // light

   /**
    * Makes error and success states more colorful.
    */
   var richColors: Boolean?

   /**
    * Toasts will be expanded by default.
    */
   var expand: Boolean?

   /**
    * Amount of visible toasts.
    */
   var visibleToasts: Int?

   /**
    *
    */
   var position: String? // "bottom-right"

   /**
    * Adds a close button to all toasts.
    */
   var closeButton: String?

   /**
    * Offset from the edges of the screen.
    */
   var offset: dynamic

   /**
    * Offset from the edges when the screen width is less than 600px.
    */
   var mobileOffset: dynamic

   /**
    * Array of swipe directions, default is based on position. bottom-left position means
    * you can swipe to the right or bottom etc.
    */
   var swipeDirections: dynamic

   /**
    * Directionality of toast's text.
    */
   var dir: String? // "ltr" | "rtl"

   /**
    * Keyboard shortcut that will move focus to the toaster area.
    */
   var hotkey: String?

   /**
    * Dark toasts in light mode and vice versa.
    */
   var invert: Boolean?

   /**
    * These will act as default options for all toasts. See toast() for all available options.
    */
   var toastOptions: ToastOptions?

   /**
    * Gap between toasts when expanded.
    */
   var gap: Int?

   /**
    * Changes the default icons.
    */
   var icons: dynamic
}

@JsName("Toaster")
external val Toaster: ComponentType<ToasterProps>

@JsName("toast")
external object toast {
   val length: Int
   fun getActiveToasts(): Int
   fun success(message: ToastMessage, options: ToastOptions? = definedExternally): String
   fun error(message: ToastMessage, options: ToastOptions? = definedExternally): String
   fun message(message: ToastMessage, options: ToastOptions? = definedExternally): String
   fun custom(content: ReactNode, options: ToastOptions? = definedExternally): String
   fun dismiss(id: String?)
}

@JsName("toast")
external fun toast(message: ToastMessage, options: ToastOptions): String

external interface Toast {
   var id: String
}

external interface UseSonner {
   var toasts: Array<Toast>
}

@JsName("useSonner")
external fun useSonner(): UseSonner