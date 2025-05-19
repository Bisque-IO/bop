@file:JsModule("@radix-ui/react-progress") @file:JsNonModule

package lib.radix

import react.ComponentType

/*

Anatomy
Import all parts and piece them together.

import { Progress } from "radix-ui";

export default () => (
	<Progress.Root>
		<Progress.Indicator />
	</Progress.Root>
);

*/

/**
 * @see ProgressPrimitive
 */
external interface ProgressRootProps : RadixProps, PropsWithAsChild {
   /**
    * The progress value.
    */
   var value: Number?

   /**
    * The maximum progress value.
    */
   var max: Number?

   /**
    * A function to get the accessible label text representing the current value in a
    * human-readable format. If not provided, the value label will be read as the numeric
    * value as a percentage of the max value.
    */
   var getValueLabel: ((value: Number, max: Number) -> String)?

   @JsName("data-state")
   var dataState: String? // "complete" | "indeterminate" | "loading"

   @JsName("data-value")
   var dataValue: String?

   @JsName("data-max")
   var dataMax: String?
}

/**
 * Contains all the progress parts.
 */
@JsName("Root")
external val ProgressPrimitive: ComponentType<ProgressRootProps>

/**
 * @see ProgressPrimitiveIndicator
 */
external interface ProgressIndicatorProps : RadixProps, PropsWithAsChild {
   @JsName("data-state")
   var dataState: String? // "complete" | "indeterminate" | "loading"

   @JsName("data-value")
   var dataValue: String?

   @JsName("data-max")
   var dataMax: String?
}

/**
 * Used to show the progress visually. It also makes progress accessible to assistive technologies.
 */
@JsName("Indicator")
external val ProgressPrimitiveIndicator: ComponentType<ProgressIndicatorProps>
