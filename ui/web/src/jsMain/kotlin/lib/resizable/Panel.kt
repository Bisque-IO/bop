@file:JsModule("react-resizable-panels") @file:JsNonModule

package lib.resizable

import react.ComponentType
import react.PropsWithChildren
import react.PropsWithClassName
import react.PropsWithStyle

external interface PanelGroupProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var autoSaveId: String?
   var direction: String? // "horizontal" | "vertical"
   var id: String?
   var onLayout: ((sizes: Array<Number>) -> Unit)?
   var storage: dynamic
   var tagName: String?
}

@JsName("PanelGroup")
external val PanelGroup: ComponentType<PanelGroupProps>

external interface PanelProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   /**
    * Panel should collapse to this size
    */
   var collapsedSize: Number?

   /**
    * Panel should collapse when resized beyond its minSize
    */
   var collapsible: Boolean?

   /**
    * Initial size of panel (numeric value between 1-100)
    */
   var defaultSize: Number?

   /**
    * Panel id (unique within group); falls back to useId when not provided
    */
   var id: String?

   /**
    * Maximum allowable size of panel (numeric value between 1-100); defaults to 100
    */
   var maxSize: Number?

   /**
    * Minimum allowable size of panel (numeric value between 1-100); defaults to 10
    */
   var minSize: Number?

   /**
    * Called when panel is collapsed
    */
   var onCollapse: (() -> Unit)?

   /**
    * Called when panel is expanded
    */
   var onExpand: (() -> Unit)?

   /**
    * Called when panel is resized; size parameter is a numeric value between 1-100. 1
    */
   var onResize: ((size: Number) -> Unit)?

   /**
    * Order of panel within group; required for groups with conditionally rendered panels
    */
   var order: Number?

   /**
    * HTML element tag name for root element
    */
   var tagName: String?
}

@JsName("Panel")
external val Panel: ComponentType<PanelProps>

external interface HitAreaMargins {
   var coarse: Number?
   var fine: Number?
}

external interface PanelResizeHandleProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   /**
    * Allow this much margin when determining resizable handle hit detection
    */
   var hitAreaMargins: HitAreaMargins?

   /**
    * Disable drag handle
    */
   var disabled: Boolean?

   /**
    * Resize handle id (unique within group); falls back to useId when not provided
    */
   var id: String?

   /**
    * Called when group layout changes
    */
   var onDragging: ((isDragging: Boolean) -> Unit)?

   /**
    * HTML element tag name for root element
    */
   var tagName: String?
}

@JsName("PanelResizeHandle")
external val PanelResizeHandle: ComponentType<PanelResizeHandleProps>