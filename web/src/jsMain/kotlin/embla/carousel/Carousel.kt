@file:JsModule("embla-carousel-react") @file:JsNonModule

package embla.carousel

import web.html.HTMLElement

sealed external interface EmblaEventListType

/*
export interface EmblaEventListType {
    init: 'init';
    pointerDown: 'pointerDown';
    pointerUp: 'pointerUp';
    slidesChanged: 'slidesChanged';
    slidesInView: 'slidesInView';
    scroll: 'scroll';
    select: 'select';
    settle: 'settle';
    destroy: 'destroy';
    reInit: 'reInit';
    resize: 'resize';
    slideFocusStart: 'slideFocusStart';
    slideFocus: 'slideFocus';
}
 */

external interface EventHandlerType {
   var `init`: (emblaApi: EmblaCarouselType) -> Unit
   var emit: (evt: dynamic) -> EventHandlerType
   var on: (evt: dynamic, cb: (emblaApi: EmblaCarouselType, evt: dynamic) -> Unit) -> EventHandlerType
   var off: (evt: dynamic, cb: (emblaApi: EmblaCarouselType, evt: dynamic) -> Unit) -> EventHandlerType
   var clear: () -> Unit
}

external interface EmblaCarouselType {
   var canScrollNext: () -> Boolean
   var canScrollPrev: () -> Boolean
   var containerNode: () -> HTMLElement
   var internalEngine: () -> dynamic
   var destroy: () -> Unit
   var off: (evt: dynamic, cb: (emblaApi: EmblaCarouselType, evt: dynamic) -> Unit) -> EventHandlerType
   var on: (evt: dynamic, cb: (emblaApi: EmblaCarouselType, evt: dynamic) -> Unit) -> EventHandlerType
   var emit: (evt: dynamic) -> EventHandlerType
   var plugins: () -> dynamic
   var previousScrollSnap: () -> Number
   var reInit: (options: EmblaOptionsType?, plugins: Array<dynamic>?) -> Unit
   var rootNode: () -> HTMLElement
   var scrollNext: (jump: Boolean?) -> Unit
   var scrollPrev: (jump: Boolean?) -> Unit
   var scrollProgress: () -> Number
   var scrollSnapList: () -> Array<Number>
   var scrollTo: (index: Number, jump: Boolean?) -> Unit
   var selectedScrollSnap: () -> Number
   var slideNodes: () -> HTMLElement
   var slidesInView: () -> Array<Number>
   var slidesNotInView: () -> Array<Number>
}

external interface EmblaOptionsType {
   var active: Boolean? // Default: true
   var align: dynamic /* String | (viewSize: Number, snapSize: Number, index: Number) -> Number */ // Default:

   // "center"
   var axis: String? // "x" or "y", Default: "x"
   var breakpoints: dynamic /* Map<String, EmblaOptionsType> */ // Default: undefined
   var container: dynamic /* String | HTMLElement | null */ // Default: null
   var containScroll: dynamic /* Boolean | String */ // "trimSnaps" | "keepSnaps" | false, Default: "trimSnaps"
   var direction: String? // "ltr" or "rtl", Default: "ltr"
   var dragFree: Boolean? // Default: false
   var dragThreshold: Number? // Default: 10
   var duration: Number? // Default: 25
   var inViewThreshold: Number? // Default: 0
   var loop: Boolean? // Default: false
   var skipSnaps: Boolean? // Default: false
   var slides: dynamic /* String | Array<HTMLElement> | NodeListOf<HTMLElement> | null */ // Default: null
   var slidesToScroll: dynamic /* String | Number */ // Default: 1
}

external interface EmblaCarouselPlugin

//@JsName("useEmblaCarousel")
@JsName("default")
external fun useEmblaCarousel(
   options: EmblaOptionsType? = definedExternally,
   plugins: Array<EmblaCarouselPlugin>? = definedExternally,
): Array<dynamic> // Returns [emblaRef, emblaApi]

//@JsName("EmblaCarousel")
//external fun EmblaCarousel(
//   root: HTMLElement,
//   userOptions: EmblaOptionsType? = definedExternally,
//   plugins: Array<EmblaCarouselPlugin>? = definedExternally
//): EmblaCarouselType