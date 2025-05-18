package bop.ui

import web.cssom.ClassName

fun cn(name: String) = ClassName(name)

fun cn(vararg inputs: dynamic): ClassName {
   return ClassName(twMerge(clsx(*inputs)))
}
