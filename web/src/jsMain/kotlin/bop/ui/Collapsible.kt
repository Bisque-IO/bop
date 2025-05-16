package bop.ui

import radix.ui.CollapsibleProps
import react.FC

val Collapsible = FC<CollapsibleProps>("Collapsible") { props ->
   radix.ui.Collapsible {
      dataSlot = "collapsible"
      spread(props)
   }
}

val CollapsibleTrigger = FC<CollapsibleProps>("CollapsibleTrigger") { props ->
   radix.ui.CollapsibleTrigger {
      dataSlot = "collapsible-trigger"
      spread(props)
   }
}

val CollapsibleContent = FC<CollapsibleProps>("CollapsibleContent") { props ->
   radix.ui.CollapsibleContent {
      dataSlot = "collapsible-content"
      spread(props)
   }
}
