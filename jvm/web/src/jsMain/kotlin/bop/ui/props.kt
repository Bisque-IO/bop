package bop.ui

import react.PropsWithChildren
import react.PropsWithClassName
import react.dom.html.HTMLAttributes
import web.html.HTMLElement

external interface VariantProps : PropsWithChildren, PropsWithClassName, HTMLAttributes<HTMLElement> {
   var variant: String?
}

external interface DefaultProps : PropsWithChildren, PropsWithClassName, HTMLAttributes<HTMLElement>

external interface AsChildProps : DefaultProps {
   var asChild: Boolean?
}
