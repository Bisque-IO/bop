package bop.ui

import radix.ui.LabelRootProps
import react.FC

external interface LabelProps : LabelRootProps

val Label = FC<LabelProps>("Label") { props ->
   radix.ui.LabelRoot {
      dataSlot = "label"
      className = cn("flex items-center gap-2 text-sm leading-none font-medium select-none group-data-[disabled=true]:pointer-events-none group-data-[disabled=true]:opacity-50 peer-disabled:cursor-not-allowed peer-disabled:opacity-50", props.className)
      spread(props, "className")
   }
}