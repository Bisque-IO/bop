@file:JsModule("@radix-ui/react-slider") @file:JsNonModule

package lib.radix

import react.ComponentType

/*

Anatomy
Import all parts and piece them together.

import { Slider } from "radix-ui";

export default () => (
	<Slider.Root>
		<Slider.Track>
			<Slider.Range />
		</Slider.Track>
		<Slider.Thumb />
	</Slider.Root>
);


Examples
Vertical orientation
Use the orientation prop to create a vertical slider.

// index.jsx
import { Slider } from "radix-ui";
import "./styles.css";

export default () => (
	<Slider.Root
		className="SliderRoot"
		defaultValue={[50]}
		orientation="vertical"
	>
		<Slider.Track className="SliderTrack">
			<Slider.Range className="SliderRange" />
		</Slider.Track>
		<Slider.Thumb className="SliderThumb" />
	</Slider.Root>
);
/* styles.css */
.SliderRoot {
	position: relative;
	display: flex;
	align-items: center;
}
.SliderRoot[data-orientation="vertical"] {
	flex-direction: column;
	width: 20px;
	height: 100px;
}

.SliderTrack {
	position: relative;
	flex-grow: 1;
	background-color: grey;
}
.SliderTrack[data-orientation="vertical"] {
	width: 3px;
}

.SliderRange {
	position: absolute;
	background-color: black;
}
.SliderRange[data-orientation="vertical"] {
	width: 100%;
}

.SliderThumb {
	display: block;
	width: 20px;
	height: 20px;
	background-color: black;
}

Create a range
Add multiple thumbs and values to create a range slider.

import { Slider } from "radix-ui";

export default () => (
	<Slider.Root defaultValue={[25, 75]}>
		<Slider.Track>
			<Slider.Range />
		</Slider.Track>
		<Slider.Thumb />
		<Slider.Thumb />
	</Slider.Root>
);
Define step size
Use the step prop to increase the stepping interval.

import { Slider } from "radix-ui";

export default () => (
	<Slider.Root defaultValue={[50]} step={10}>
		<Slider.Track>
			<Slider.Range />
		</Slider.Track>
		<Slider.Thumb />
	</Slider.Root>
);

Prevent thumb overlap
Use minStepsBetweenThumbs to avoid thumbs with equal values.

import { Slider } from "radix-ui";

export default () => (
	<Slider.Root defaultValue={[25, 75]} step={10} minStepsBetweenThumbs={1}>
		<Slider.Track>
			<Slider.Range />
		</Slider.Track>
		<Slider.Thumb />
		<Slider.Thumb />
	</Slider.Root>
);

Accessibility
Adheres to the Slider WAI-ARIA design pattern.

Keyboard Interactions
Key	               Description
ArrowRight           Increments/decrements by the step value depending on orientation.
ArrowLeft            Increments/decrements by the step value depending on orientation.
ArrowUp              Increases the value by the step amount.
ArrowDown            Decreases the value by the step amount.
PageUp               Increases the value by a larger step.
PageDown             Decreases the value by a larger step.
Shift+ArrowUp        Increases the value by a larger step.
Shift+ArrowDown      Decreases the value by a larger step.
Home                 Sets the value to its minimum.
End                  Sets the value to its maximum.

Custom APIs
Create your own API by abstracting the primitive parts into your own component.

Abstract all parts
This example abstracts all the Slider parts so it can be used as a self-closing element.

Usage
import { Slider } from "./your-slider";

export default () => <Slider defaultValue={[25]} />;
Implementation
// your-slider.jsx
import { Slider as SliderPrimitive } from "radix-ui";

export const Slider = React.forwardRef((props, forwardedRef) => {
	const value = props.value || props.defaultValue;

	return (
		<SliderPrimitive.Slider {...props} ref={forwardedRef}>
			<SliderPrimitive.Track>
				<SliderPrimitive.Range />
			</SliderPrimitive.Track>
			{value.map((_, i) => (
				<SliderPrimitive.Thumb key={i} />
			))}
		</SliderPrimitive.Slider>
	);
});
Caveats
Mouse events are not fired
Because of a limitation we faced during implementation, the following example won't work
as expected and the onMouseDown and onMouseUp event handlers won't be fired:

<Slider.Root
	onMouseDown={() => console.log("onMouseDown")}
	onMouseUp={() => console.log("onMouseUp")}
>
	…
</Slider.Root>

We recommend using pointer events instead (eg. onPointerDown, onPointerUp). Regardless of the above
limitation, these events are better suited for cross-platform/device handling as they are fired for
all pointer input types (mouse, touch, pen, etc.).
*/

/**
 * @see SliderPrimitiveRoot
 */
external interface SliderRootProps : RadixProps, PropsWithAsChild {
   /**
    * The value of the slider when initially rendered. Use when you do
    * not need to control the state of the slider.
    */
   var defaultValue: Array<Number>?

   /**
    * The controlled value of the slider. Must be used in conjunction with onValueChange.
    */
   var value: Array<Number>?

   /**
    * Event handler called when the value changes.
    */
   var onValueChange: ((value: Array<Number>) -> Unit)?

   /**
    * Event handler called when the value changes at the end of an interaction. Useful when
    * you only need to capture a final value e.g. to update a backend service.
    */
   var onValueCommit: ((value: Array<Number>) -> Unit)?

   /**
    * The name of the slider. Submitted with its owning form as part of a name/value pair.
    */
   var name: String?

   /**
    * When true, prevents the user from interacting with the slider.
    */
   var disabled: Boolean?

   /**
    * The orientation of the slider.
    */
   var orientation: HorizontalOrVertical? // "horizontal" | "vertical"

   /**
    * The reading direction of the slider. If omitted, inherits globally from
    * DirectionProvider or assumes LTR (left-to-right) reading mode.
    */
   var dir: TextDirection? // "ltr" | "rtl"

   /**
    * Whether the slider is visually inverted.
    */
   var inverted: Boolean?

   /**
    * The minimum value for the range.
    */
   var min: Number?

   /**
    * The maximum value for the range.
    */
   var max: Number?

   /**
    * The stepping interval.
    */
   var step: Number?

   /**
    * The minimum permitted steps between multiple thumbs.
    */
   var minStepsBetweenThumbs: Int?

   /**
    * The ID of the form that the slider belongs to. If omitted, the
    * slider will be associated with a parent form if one exists.
    */
   var form: String?

   @JsName("data-disabled")
   var dataDisabled: Boolean?

   @JsName("data-orientation")
   var dataOrientation: HorizontalOrVertical
}

/**
 * Contains all the parts of a slider. It will render an input for each
 * thumb when used within a form to ensure events propagate correctly.
 */
@JsName("Root")
external val SliderPrimitiveRoot: ComponentType<SliderRootProps>

/**
 * @see SliderPrimitiveTrack
 */
external interface SliderTrackProps : RadixProps, PropsWithAsChild {
   @JsName("data-disabled")
   var dataDisabled: Boolean?

   @JsName("data-orientation")
   var dataOrientation: HorizontalOrVertical
}

/**
 * The track that contains the Slider.Range.
 */
@JsName("Track")
external val SliderPrimitiveTrack: ComponentType<SliderTrackProps>

/**
 * @see SliderPrimitiveRange
 */
external interface SliderRangeProps : RadixProps, PropsWithAsChild {
   @JsName("data-disabled")
   var dataDisabled: Boolean?

   @JsName("data-orientation")
   var dataOrientation: HorizontalOrVertical
}

/**
 * The range part. Must live inside SliderTrack.
 */
@JsName("Range")
external val SliderPrimitiveRange: ComponentType<SliderRangeProps>

/**
 * @see SliderPrimitiveThumb
 */
external interface SliderThumbProps : RadixProps, PropsWithAsChild {
   @JsName("data-disabled")
   var dataDisabled: Boolean?

   @JsName("data-orientation")
   var dataOrientation: HorizontalOrVertical
}

/**
 * A draggable thumb. You can render multiple thumbs.
 */
@JsName("Thumb")
external val SliderPrimitiveThumb: ComponentType<SliderThumbProps>
