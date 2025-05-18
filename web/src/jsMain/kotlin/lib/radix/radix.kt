package lib.radix

import react.FC
import react.PropsWithChildren
import react.PropsWithClassName
import react.PropsWithStyle

external interface RadixProps : PropsWithChildren, PropsWithClassName, PropsWithStyle

external interface AccordionProps : PropsWithChildren

external interface ButtonProps : PropsWithChildren {
    var asChild: Boolean
    var size: String
    var color: String
    var highContrast: Boolean
    var radius: String
    var loading: Boolean
}

@JsModule("@radix-ui/react-accordion")
@JsNonModule
external object AccordionModule {
    val Accordion: FC<AccordionProps>
}

@JsModule("@radix-ui/react-accordion")
@JsNonModule
external val Accordion: dynamic

@JsModule("@radix-ui/react-accordion")
@JsNonModule
external fun createAccordionScope(): dynamic
