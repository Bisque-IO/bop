package bop.ui

import lib.embla.carousel.EmblaCarouselPlugin
import lib.embla.carousel.EmblaCarouselType
import lib.embla.carousel.EmblaOptionsType
import lib.embla.carousel.useEmblaCarousel
import js.objects.unsafeJso
import react.FC
import react.RefCallback
import react.dom.aria.AriaRole
import react.dom.events.KeyboardEvent
import react.dom.html.ReactHTML.div
import react.useCallback
import react.useEffect
import react.useEffectWithCleanup
import react.useState
import web.html.HTMLDivElement
import web.html.HTMLElement
import kotlin.js.unsafeCast


external interface CarouselProps : DefaultProps {
   var opts: EmblaOptionsType
   var plugins: EmblaCarouselPlugin
   var orientation: String // 'horizontal' | 'vertical'
   var setApi: ((api: dynamic) -> Unit)?
}

external interface CarouselContextProps : CarouselProps {
   var carouselRef: dynamic
   var api: dynamic
   var scrollPrev: (() -> Unit)?
   var scrollNext: (() -> Unit)?
   var canScrollPrev: Boolean?
   var canScrollNext: Boolean?
}

val CarouselContext = react.createContext<CarouselContextProps>()

fun useCarousel(): CarouselContextProps {
   val context = react.use(CarouselContext)
   if (context == null) {
      throw Error("useCarousel must be used within a <Carousel />")
   }
   return context
}


val Carousel = FC<CarouselProps>("Carousel") { props ->
   val result = useEmblaCarousel(unsafeJso {
      spread(props.opts)
      axis = if (props.orientation == "horizontal") "x" else "y"
   })
   val carouselRef = result[0].unsafeCast<RefCallback<HTMLElement>>()
   val api = result[1]?.unsafeCast<EmblaCarouselType?>()

   val (canScrollPrev, setCanScrollPrev) = useState(false)
   val (canScrollNext, setCanScrollNext) = useState(false)

   val onSelect = useCallback { api: EmblaCarouselType? ->
      if (api != null) {
         setCanScrollPrev(api.canScrollPrev())
         setCanScrollNext(api.canScrollNext())
      }
   }

   val scrollPrev = useCallback(api) {
      api?.scrollPrev(false) ?: Unit
   }
   val scrollNext = useCallback(api) {
      api?.scrollNext(false) ?: Unit
   }

   val handleKeyDown = useCallback(scrollPrev, scrollNext) { event: KeyboardEvent<HTMLDivElement> ->
      if (event.key == "ArrowLeft") {
         event.preventDefault()
         scrollPrev()
      } else if (event.key == "ArrowRight") {
         event.preventDefault()
         scrollNext()
      }
   }

   useEffect(api, props.setApi) {
      if (api != null && props.setApi != null) {
         props.setApi?.invoke(api)
      }
   }

   useEffectWithCleanup(api, onSelect) {
      if (api != null) {
         api.on("reInit", onSelect.asDynamic())
         api.on("select", onSelect.asDynamic())
         onCleanup {
            api.off("select", onSelect.asDynamic())
         }
      }
   }

   CarouselContext.Provider {
      value = unsafeJso {
         this.carouselRef = carouselRef
         this.api = api
         this.opts = opts
         this.orientation = if (opts.axis == "y") "vertical" else "horizontal"
         this.scrollPrev = scrollPrev
         this.scrollNext = scrollNext
         this.canScrollPrev = canScrollPrev
         this.canScrollNext = canScrollNext
      }
      div {
         spread(props, "className")
         dataSlot = "carousel"
         onKeyDownCapture = { handleKeyDown(it) }
         className = cn("relative", props.className)
         role = AriaRole.region
         ariaRoleDescription = "carousel"
      }
   }
}
