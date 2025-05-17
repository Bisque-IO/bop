@file:JsModule("@radix-ui/react-avatar") @file:JsNonModule

package radix.ui

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
 * @see AvatarRoot
 */
external interface AvatarRootProps : DefaultProps, PropsWithAsChild

/**
 * Contains all the parts of an avatar.
 */
@JsName("Root")
external val AvatarRoot: ComponentType<AvatarRootProps>

/**
 * @see AvatarImage
 */
external interface AvatarImageProps : DefaultProps, PropsWithAsChild {
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
external val AvatarImage: ComponentType<AvatarImageProps>

/**
 * @see AvatarFallback
 */
external interface AvatarFallbackProps : DefaultProps, PropsWithAsChild {
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
external val AvatarFallback: ComponentType<AvatarFallbackProps>
