@file:JsModule("cmdk") @file:JsNonModule

package lib.cmdk

import react.*
import web.html.HTMLElement

external interface DefaultProps : PropsWithChildren, PropsWithClassName, PropsWithStyle

@JsName("Command")
external val Command: ComponentType<CommandProps>

external interface CommandProps : DefaultProps {
   var value: String?
   var onValueChange: ((String) -> Unit)?
   var filter: ((value: String, search: String, keywords: Array<String>?) -> Int)?
   var shouldFilter: Boolean?
   var loop: Boolean?
   var label: String?
   var ref: Ref<HTMLElement>?
}

@JsName("CommandInput")
external val CommandInput: ComponentType<CommandInputProps>

external interface CommandInputProps : DefaultProps {
   var value: String?
   var onValueChange: ((String) -> Unit)?
   var placeholder: String?
   var autoFocus: Boolean?
   var ref: Ref<HTMLElement>?
}

@JsName("CommandList")
external val CommandList: ComponentType<CommandListProps>

external interface CommandListProps : DefaultProps {
   var ref: Ref<HTMLElement>?
}

@JsName("CommandItem")
external val CommandItem: ComponentType<CommandItemProps>

external interface CommandItemProps : DefaultProps {
   var value: String?
   var disabled: Boolean?
   var onSelect: (() -> Unit)?
   var keywords: Array<String>?
   var forceMount: Boolean?
   var ref: Ref<HTMLElement>?
}

@JsName("CommandGroup")
external val CommandGroup: ComponentType<CommandGroupProps>

external interface CommandGroupProps : DefaultProps {
   var heading: String?
   var forceMount: Boolean?
   var ref: Ref<HTMLElement>?
}

@JsName("CommandSeparator")
external val CommandSeparator: ComponentType<CommandSeparatorProps>

external interface CommandSeparatorProps : DefaultProps {
   var alwaysRender: Boolean?
   var ref: Ref<HTMLElement>?
}

@JsName("CommandEmpty")
external val CommandEmpty: ComponentType<CommandEmptyProps>

external interface CommandEmptyProps : DefaultProps {
   var ref: Ref<HTMLElement>?
}

@JsName("CommandLoading")
external val CommandLoading: ComponentType<CommandLoadingProps>

external interface CommandLoadingProps : DefaultProps {
   var ref: Ref<HTMLElement>?
}

@JsName("CommandDialog")
external val CommandDialog: ComponentType<CommandDialogProps>

external interface CommandDialogProps : CommandProps {
   var open: Boolean?
   var onOpenChange: ((Boolean) -> Unit)?
   var container: HTMLElement?
}