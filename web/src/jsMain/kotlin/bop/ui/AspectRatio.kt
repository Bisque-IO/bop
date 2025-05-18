package bop.ui

import lib.radix.AspectRatioPrimitiveRoot
import lib.radix.AspectRatioProps
import react.FC

val AspectRatio = FC<AspectRatioProps>("AspectRatio") { props ->
   AspectRatioPrimitiveRoot {
      spread(props)
      dataSlot = "aspect-ratio"
   }
}
