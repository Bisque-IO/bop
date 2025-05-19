package bop.ui

import lib.radix.CollapsiblePrimitiveContent
import lib.radix.CollapsiblePrimitiveRoot
import lib.radix.CollapsiblePrimitiveTrigger
import lib.radix.CollapsibleRootProps
import react.FC

val Collapsible = FC<CollapsibleRootProps>("Collapsible") { props ->
   CollapsiblePrimitiveRoot {
      +props
      dataSlot = "collapsible"
   }
}

val CollapsibleTrigger = FC<CollapsibleRootProps>("CollapsibleTrigger") { props ->
   CollapsiblePrimitiveTrigger {
      +props
      dataSlot = "collapsible-trigger"
   }
}

val CollapsibleContent = FC<CollapsibleRootProps>("CollapsibleContent") { props ->
   CollapsiblePrimitiveContent {
      +props
      dataSlot = "collapsible-content"
   }
}
