package bop.ui

import lib.lucide.CheckIcon
import lib.radix.CheckboxPrimitiveIndicator
import lib.radix.CheckboxPrimitiveRoot
import lib.radix.CheckboxRootProps
import react.FC
import web.cssom.ClassName

val Checkbox = FC<CheckboxRootProps>("Checkbox") { props ->
   CheckboxPrimitiveRoot {
      spread(props, "className", "children")
      dataSlot = "checkbox"
      className = cn(
         "peer border-input data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground data-[state=checked]:border-primary focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive size-4 shrink-0 rounded-[4px] border shadow-xs transition-shadow outline-none focus-visible:ring-[3px] disabled:cursor-not-allowed disabled:opacity-50",
         props.className
      )
      CheckboxPrimitiveIndicator {
         dataSlot = "checkbox-indicator"
         className = ClassName("flex items-center justify-center text-current transition-none")
         CheckIcon {
            className = ClassName("size-3.5")
         }
      }
   }
}
