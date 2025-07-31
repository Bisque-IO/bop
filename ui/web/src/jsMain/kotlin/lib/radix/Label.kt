@file:JsModule("@radix-ui/react-label")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

Anatomy
Import the component.

import { Label } from "radix-ui";

export default () => <Label.Root />;


Accessibility
This component is based on the native label element, it will automatically apply
the correct labelling when wrapping controls or using the htmlFor attribute.
For your own custom controls to work correctly, ensure they use native elements
such as button or input as a base.

*/

/**
 * @see LabelPrimitiveRoot
 */
external interface LabelRootProps : RadixProps, PropsWithAsChild {
    /**
     * The id of the element the label is associated with.
     */
    var htmlFor: String?
}

@JsName("Root")
external val LabelPrimitiveRoot: ComponentType<LabelRootProps>
