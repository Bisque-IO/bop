package bop.ui

import radix.ui.*
import react.FC
import react.dom.html.ReactHTML.div
import web.cssom.ClassName

val Accordion = FC<AccordionProps>("Accordion") { props ->
   AccordionRoot {
      asDynamic()["data-slot"] = "accordion"
      +props.children
   }
}

val AccordionItem = FC<AccordionItemProps>("AccordionItem") { props ->
   radix.ui.AccordionItem {
      this["data-slot"] = "accordion-item"
      className = cn("border-b last:border-b-0", props.className)
      spread(props, "className")
   }
}

val AccordionTrigger = FC<AccordionTriggerProps>("AccordionTrigger") { props ->
   AccordionHeader {
      radix.ui.AccordionTrigger {
         this["data-slot"] = "accordion-trigger"
         className = cn(
            "focus-visible:border-ring focus-visible:ring-ring/50 flex flex-1 items-start justify-between gap-4 rounded-md py-4 text-left text-sm font-medium transition-all outline-none hover:underline focus-visible:ring-[3px] disabled:pointer-events-none disabled:opacity-50 [&[data-state=open]>svg]:rotate-180",
            props.className,
         )
         spread(props, "className")
         ChevronDownIcon {
            className = ClassName(
               "text-muted-foreground pointer-events-none size-4 shrink-0 translate-y-0.5 transition-transform duration-200"
            )
         }
      }
   }
}

val AccordionContent = FC<AccordionContentProps>("AccordionContent") { props ->
   radix.ui.AccordionContent {
      this["data-slot"] = "accordion-content"
      className = ClassName(
         "data-[state=closed]:animate-accordion-up data-[state=open]:animate-accordion-down overflow-hidden text-sm"
      )
      spread(props, ExcludeSets.CLASS_NAME_CHILDREN)
      div {
         className = cn("pt-0 pb-4", props.className)
         +props.children
      }
   }
}
