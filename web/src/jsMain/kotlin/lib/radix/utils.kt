package lib.radix

import js.objects.unsafeJso
import js.reflect.unsafeCast
import react.Props
import react.PropsWithChildren
import react.PropsWithClassName
import react.PropsWithStyle
import web.cssom.Length
import web.cssom.LengthProperty
import web.dom.Element
import web.html.HTMLElement

/*

Composition
Use the asChild prop to compose Radix's functionality onto alternative
element types or your own React .components.

All Radix primitive parts that render a DOM element accept an asChild prop.
When asChild is set to true, Radix will not render a default DOM element,
instead cloning the part's child and passing it the props and behavior
required to make it functional.

Changing the element type
In the majority of cases you shouldn’t need to modify the element type as
Radix has been designed to provide the most appropriate defaults. However,
there are cases where it is helpful to do so.

A good example is with Tooltip.Trigger. By default, this part is rendered
as a button, though you may want to add a tooltip to a link (a tag) as well.
Let's see how you can achieve this using asChild:

import * as React from "react";
import { Tooltip } from "radix-ui";

export default () => (
	<Tooltip.Root>
		<Tooltip.Trigger asChild>
			<a href="https://www.radix-ui.com/">Radix UI</a>
		</Tooltip.Trigger>
		<Tooltip.Portal>…</Tooltip.Portal>
	</Tooltip.Root>
);
If you do decide to change the underlying element type, it is your responsibility
to ensure it remains accessible and functional. In the case of Tooltip.Trigger for
example, it must be a focusable element that can respond to pointer and keyboard
events. If you were to switch it to a div, it would no longer be accessible.

In reality, you will rarely modify the underlying DOM element like we've seen above.
Instead, it's more common to use your own React .components. This is especially true
for most Trigger parts, as you usually want to compose the functionality with the
custom buttons and links in your design system.

Composing with your own React .components
This works exactly the same as above, you pass asChild to the part and then wrap
your own component with it. However, there are a few gotchas to be aware of.

Your component must spread props
When Radix clones your component, it will pass its own props and event handlers to
make it functional and accessible. If your component doesn't support those props,
it will break.

This is done by spreading all the props onto the underlying DOM node.

// before
const MyButton = () => <button />;

// after
const MyButton = (props) => <button {...props} />;
We recommend always doing this so that you are not concerned with implementation
details (ie. which props/events to accept). We find this is good practice for "leaf"
.components in general.

Similarly to when changing the element type directly, it is your responsibility to
ensure the element type rendered by your custom component remains accessible and
functional.

Your component must forward ref
Additionally, Radix will sometimes need to attach a ref to your component
(for example to measure its size). If your component doesn't accept a ref,
then it will break.

This is done using React.forwardRef (read more on react.dev).

// before
const MyButton = (props) => <button {...props} />;

// after
const MyButton = React.forwardRef((props, forwardedRef) => (
	<button {...props} ref={forwardedRef} />
));
Whilst this isn't necessary for all parts, we recommend always doing it so that you
are not concerned with implementation details. This is also generally good practice
anyway for leaf .components.

Composing multiple primitives
asChild can be used as deeply as you need to. This means it is a great way to compose
multiple primitive's behavior together. Here is an example of how you can compose
TooltipTrigger and DialogTrigger together with your own button:

import * as React from "react";
import { Dialog, Tooltip } from "radix-ui";

const MyButton = React.forwardRef((props, forwardedRef) => (
	<button {...props} ref={forwardedRef} />
));

export default () => {
	return (
		<Dialog.Root>
			<Tooltip.Root>
				<Tooltip.Trigger asChild>
					<Dialog.Trigger asChild>
						<MyButton>Open dialog</MyButton>
					</Dialog.Trigger>
				</Tooltip.Trigger>
				<Tooltip.Portal>…</Tooltip.Portal>
			</Tooltip.Root>

			<Dialog.Portal>...</Dialog.Portal>
		</Dialog.Root>
	);
};

 */

external interface PropsWithAsChild : Props {
   /**
    * Change the default rendered element for the one passed as a child, merging their props and behavior.
    *
    * Read our Composition guide for more details.
    */
   var asChild: Boolean?
}

external interface RadixProps : PropsWithChildren, PropsWithClassName, PropsWithStyle

external interface CollisionBoundary

fun CollisionBoundary(element: Element) = unsafeCast<CollisionBoundary>(element)

fun CollisionBoundary(vararg elements: Element) = unsafeCast<CollisionBoundary>(elements)

fun CollisionBoundary(elements: Array<Element>) = unsafeCast<CollisionBoundary>(elements)

external interface CollisionPadding {
   var top: Int
   var left: Int
   var bottom: Int
   var right: Int
}

fun CollisionPadding(
   padding: Int,
) = CollisionPadding(padding, padding, padding, padding)

fun CollisionPadding(
   top: Int,
   right: Int,
   bottom: Int,
   left: Int,
): CollisionPadding =
   unsafeJso {
      this.top = top
      this.right = right
      this.bottom = bottom
      this.left = left
   }

sealed interface Direction {
   companion object {
      val ltr: Direction = unsafeCast("ltr")
      val rtl: Direction = unsafeCast("rtl")
   }
}

sealed interface OpenOrClosed {
   companion object {
      val open: OpenOrClosed = unsafeCast("open")
      val closed: OpenOrClosed = unsafeCast("closed")
   }
}

sealed interface CheckboxState {
   companion object {
      val checked: CheckboxState = unsafeCast("checked")
      val unchecked: CheckboxState = unsafeCast("unchecked")
      val indeterminate: CheckboxState = unsafeCast("indeterminate")
   }
}

sealed interface Side {
   companion object {
      val top: Side = unsafeCast("top")
      val right: Side = unsafeCast("right")
      val bottom: Side = unsafeCast("bottom")
      val left: Side = unsafeCast("left")
   }
}

sealed interface SelectPosition {
   companion object {
      val itemAligned: Side = unsafeCast("item-aligned")
      val popper: Side = unsafeCast("popper")
   }
}

sealed interface Align {
   companion object {
      val start: Align = unsafeCast("start")
      val center: Align = unsafeCast("center")
      val end: Align = unsafeCast("end")
   }
}

sealed interface Sticky {
   companion object {
      val partial: Sticky = unsafeCast("partial")
      val always: Sticky = unsafeCast("always")
   }
}
