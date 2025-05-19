package bop.ui

import lib.lucide.CircleIcon
import lib.radix.RadioGroupItemProps
import lib.radix.RadioGroupPrimitiveIndicator
import lib.radix.RadioGroupPrimitiveItem
import lib.radix.RadioGroupPrimitiveRoot
import lib.radix.RadioGroupProps
import react.FC

val RadioGroup = FC<RadioGroupProps>("RadioGroup") { props ->
   RadioGroupPrimitiveRoot {
      +props
      dataSlot = "radio-group"
      className = cn("grid gap-3", props.className)
   }
}

val RadioGroupItem = FC<RadioGroupItemProps>("RadioGroupItem") { props ->
   RadioGroupPrimitiveItem {
      +props
      children = null
      dataSlot = "radio-group-item"
      className = cn("border-input text-primary focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:bg-input/30 aspect-square size-4 shrink-0 rounded-full border shadow-xs transition-[color,box-shadow] outline-none focus-visible:ring-[3px] disabled:cursor-not-allowed disabled:opacity-50", props.className)

      RadioGroupPrimitiveIndicator {
         dataSlot = "radio-group-indicator"
         className = cn("relative flex items-center justify-center")

         CircleIcon {
            className = cn("fill-primary absolute top-1/2 left-1/2 size-2 -translate-x-1/2 -translate-y-1/2")
         }
      }
   }
}