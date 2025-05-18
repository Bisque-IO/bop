package bop.ui

import lib.radix.CollapsiblePrimitiveContent
import lib.radix.CollapsiblePrimitiveRoot
import lib.radix.CollapsiblePrimitiveTrigger
import lib.radix.CollapsibleRootProps
import react.FC

val Collapsible = FC<CollapsibleRootProps>("Collapsible") { props ->
   CollapsiblePrimitiveRoot {
      spread(props)
      dataSlot = "collapsible"
   }
}

val CollapsibleTrigger = FC<CollapsibleRootProps>("CollapsibleTrigger") { props ->
   CollapsiblePrimitiveTrigger {
      spread(props)
      dataSlot = "collapsible-trigger"
   }
}

val CollapsibleContent = FC<CollapsibleRootProps>("CollapsibleContent") { props ->
   CollapsiblePrimitiveContent {
      spread(props)
      dataSlot = "collapsible-content"
   }
}
