package bop.ui

import radix.ui.AspectRatioProps
import react.FC

val AspectRatio = FC<AspectRatioProps>("AspectRatio") { props ->
   radix.ui.AspectRatioRoot {
      this["data-slot"] = "aspect-ratio"
      spread(props)
   }
}
