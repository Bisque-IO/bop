package bop.ui

import radix.ui.CollapsibleRootProps
import react.FC

val Collapsible = FC<CollapsibleRootProps>("Collapsible") { props ->
   radix.ui.CollapsibleRoot {
      dataSlot = "collapsible"
      spread(props)
   }
}

val CollapsibleTrigger = FC<CollapsibleRootProps>("CollapsibleTrigger") { props ->
   radix.ui.CollapsibleTrigger {
      dataSlot = "collapsible-trigger"
      spread(props)
   }
}

val CollapsibleContent = FC<CollapsibleRootProps>("CollapsibleContent") { props ->
   radix.ui.CollapsibleContent {
      dataSlot = "collapsible-content"
      spread(props)
   }
}
