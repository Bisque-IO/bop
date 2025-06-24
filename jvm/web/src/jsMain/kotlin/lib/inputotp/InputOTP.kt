@file:JsModule("input-otp") @file:JsNonModule

package lib.inputotp

import react.ComponentType
import react.Context
import react.PropsWithChildren
import react.PropsWithClassName
import react.PropsWithStyle
import react.dom.FormAction
import react.dom.html.Capture
import react.dom.html.HTMLAttributes
import web.autofill.AutoFill
import web.cssom.ClassName
import web.form.FormEncType
import web.form.FormMethod
import web.html.HTMLInputElement
import web.html.InputType
import web.window.WindowName

external interface OTPInputProps : HTMLAttributes<HTMLInputElement>, PropsWithChildren, PropsWithClassName,
   PropsWithStyle {
   var accept: String?
   var alt: String?
   var autoComplete: AutoFill?
   var capture: Capture?
   var checked: Boolean?
   var disabled: Boolean?
   var form: String?
   var formAction: FormAction?
   var formEncType: FormEncType?
   var formMethod: FormMethod?
   var formNoValidate: Boolean?
   var formTarget: WindowName?
   var height: Double?
   var list: String?
   var max: Any? /* number | Date */
   var maxLength: Int?
   var min: Any? /* number | Date */
   var minLength: Int?
   var multiple: Boolean?
   var name: String?
   var pattern: String?
   var placeholder: String?
   var readOnly: Boolean?
   var required: Boolean?
   var size: Int?
   var src: String?
   var step: Double?
   var type: InputType?
   var width: Double?
   var value: String? // string | readonly string[] | number
   var onChange: (String) -> Unit
   var textAlign: String? // "left" | "center" | "right"
   var onComplete: ((args: Array<Any>) -> Unit)?
   var pushPasswordManagerStrategy: String? // "increase-width" | "none"
   var pasteTransformer: ((pasted: String) -> String)?
   var containerClassName: ClassName?
   var noScriptCSSFallback: String?
   var render: ((props: RenderProps) -> HTMLInputElement)?
}

external interface RenderProps {
   var slots: Array<SlotProps>
   var isFocused: Boolean
   var isHovering: Boolean
}

external interface SlotProps : PropsWithChildren, PropsWithClassName, PropsWithStyle {
   var isActive: Boolean
   var char: String?
   var placeholderChar: String?
   var hasFakeCaret: Boolean
}

@JsName("OTPInput")
external val OTPInput: ComponentType<OTPInputProps>

@JsName("OTPInputContext")
external val OTPInputContext: Context<RenderProps>

@JsName("REGEXP_ONLY_DIGITS")
external val REGEXP_ONLY_DIGITS: String
