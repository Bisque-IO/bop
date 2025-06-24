package bop.ui

import lib.clsx.clsx
import lib.tailwindMerge.twMerge
import react.PropsWithClassName
import web.cssom.ClassName

fun cn(name: String) = ClassName(name)

fun cn(vararg inputs: dynamic): ClassName {
   return ClassName(twMerge(clsx(*inputs)))
}

fun <P: PropsWithClassName> P.className(className: String) {
   this.className = cn(className)
}