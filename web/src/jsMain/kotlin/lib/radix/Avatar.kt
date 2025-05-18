@file:JsModule("@radix-ui/react-avatar") @file:JsNonModule

package lib.radix

import react.ComponentType

/*

Examples

Clickable Avatar with tooltip
You can compose the Avatar with a Tooltip to display extra information.

import { Avatar, Tooltip } from "radix-ui";

export default () => (
	<Tooltip.Root>
		<Tooltip.Trigger>
			<Avatar.Root>…</Avatar.Root>
		</Tooltip.Trigger>

		<Tooltip.Content side="top">
			Tooltip content
			<Tooltip.Arrow />
		</Tooltip.Content>
	</Tooltip.Root>
);

*/

/**
 * @see AvatarPrimitiveRoot
 */
external interface AvatarRootProps : RadixProps, PropsWithAsChild

/**
 * Contains all the parts of an avatar.
 */
@JsName("Root")
external val AvatarPrimitiveRoot: ComponentType<AvatarRootProps>

/**
 * @see AvatarPrimitiveImage
 */
external interface AvatarImageProps : RadixProps, PropsWithAsChild {
   /**
    * A callback providing information about the loading status of the image.
    * This is useful in case you want to control more precisely what to render
    * as the image is loading.
    */
   var onLoadingStatusChange: ((String) -> Unit)? // "idle" | "loading" | "loaded" | "error"
}

/**
 * The image to render. By default, it will only render when it has loaded. You can use
 * the onLoadingStatusChange handler if you need more control.
 */
@JsName("Image")
external val AvatarPrimitiveImage: ComponentType<AvatarImageProps>

/**
 * @see AvatarPrimitiveFallback
 */
external interface AvatarFallbackProps : RadixProps, PropsWithAsChild {
   /**
    * Useful for delaying rendering so it only appears for those with slower connections.
    */
   var delayMs: Int?
}

/**
 * An element that renders when the image hasn't loaded. This means whilst it's loading,
 * or if there was an error. If you notice a flash during loading, you can provide a
 * delayMs prop to delay its rendering so it only renders for those with slower connections.
 * For more control, use the onLoadingStatusChange handler on Avatar.Image.
 */
@JsName("Fallback")
external val AvatarPrimitiveFallback: ComponentType<AvatarFallbackProps>
