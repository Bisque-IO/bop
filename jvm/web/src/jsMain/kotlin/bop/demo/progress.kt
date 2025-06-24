package bop.demo

import bop.ui.Progress
import bop.ui.cn
import react.FC
import react.useEffectWithCleanup
import react.useState
import web.timers.clearTimeout
import web.timers.setTimeout

val ProgressDemo = FC {
   val (progress, setProgress) = useState(13)

   useEffectWithCleanup {
      val timer = setTimeout({ setProgress(66) }, 500)
      onCleanup {
         clearTimeout(timer)
      }
   }

   Progress {
      value = progress
      className = cn("w-[60%]")
   }
}