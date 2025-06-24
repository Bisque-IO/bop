package bop.ui

import js.objects.unsafeJso
import lib.radix.ProgressPrimitive
import lib.radix.ProgressPrimitiveIndicator
import lib.radix.ProgressRootProps
import react.FC
import web.cssom.Transform

typealias ProgressProps = ProgressRootProps

val Progress = FC<ProgressRootProps>("Progress") { props ->
   ProgressPrimitive {
      +props
      dataSlot = "progress"
      className = cn("bg-primary/20 relative h-2 w-full overflow-hidden rounded-full", props.className)

      ProgressPrimitiveIndicator {
         dataSlot = "progress-indicator"
         className = cn("bg-primary h-full w-full flex-1 transition-all")
         style = unsafeJso {
            transform = "translateX(-${100 - (props.value?.toInt() ?: 0)}%)".unsafeCast<Transform>()
         }
      }
   }
}