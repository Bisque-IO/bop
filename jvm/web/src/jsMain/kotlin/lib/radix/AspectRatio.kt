@file:JsModule("@radix-ui/react-aspect-ratio") @file:JsNonModule

package lib.radix

import react.ComponentType

/*

import { AspectRatio } from "radix-ui";

export default () => <AspectRatio.Root />;

*/

/**
 * @see AspectRatioPrimitiveRoot
 */
external interface AspectRatioProps : RadixProps, PropsWithAsChild {
   /**
    * The desired ratio.
    */
   var ratio: Double? // e.g. 16.0 / 9.0
}

/**
 * Contains the content you want to constrain to a given ratio.
 */
@JsName("Root")
external val AspectRatioPrimitiveRoot: ComponentType<AspectRatioProps>
