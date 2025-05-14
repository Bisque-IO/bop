package bop.ui.utils

import web.cssom.ClassName

fun cn(vararg inputs: dynamic): ClassName {
    return ClassName(twMerge(clsx(*inputs)))
}
