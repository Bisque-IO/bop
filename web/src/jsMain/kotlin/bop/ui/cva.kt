@file:JsModule("class-variance-authority") @file:JsNonModule

package bop.ui

import js.array.ReadonlyArray
import js.objects.Record

@JsName("cva")
external fun cva(base: String?, config: dynamic): dynamic

external interface CVAConfig {
   var variants: Record<String, Record<String, dynamic>>?
   var defaultVariants: Record<String, dynamic>?
   var compoundVariants: ReadonlyArray<CVACompoundVariant>?
}

external interface CVACompoundVariant {
   var `class`: String
   var variants: Record<String, dynamic>
}

external interface CVAFunction {
   operator fun invoke(
      props: dynamic = definedExternally,
      options: dynamic = definedExternally,
   ): String
}
