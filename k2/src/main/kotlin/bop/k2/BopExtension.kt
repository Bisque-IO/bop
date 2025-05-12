package bop.k2

import org.jetbrains.kotlin.fir.FirSession
import org.jetbrains.kotlin.fir.expressions.FirFunctionCall
import org.jetbrains.kotlin.fir.extensions.FirExtensionApiInternals
import org.jetbrains.kotlin.fir.extensions.FirExtensionRegistrar
import org.jetbrains.kotlin.fir.extensions.FirFunctionCallRefinementExtension
import org.jetbrains.kotlin.fir.resolve.calls.candidate.CallInfo
import org.jetbrains.kotlin.fir.symbols.impl.FirNamedFunctionSymbol

class BopFirRegistrar : FirExtensionRegistrar() {
    @OptIn(FirExtensionApiInternals::class)
    override fun ExtensionRegistrarContext.configurePlugin() {
        +::BopExtension
    }
}

//@OptIn(DirectDeclarationsAccess::class)
@OptIn(FirExtensionApiInternals::class)
class BopExtension(session: FirSession) : FirFunctionCallRefinementExtension(session) {
    override fun intercept(
        callInfo: CallInfo, symbol: FirNamedFunctionSymbol
    ): CallReturnType? {
        TODO("Not yet implemented")
    }

    override fun transform(
        call: FirFunctionCall, originalSymbol: FirNamedFunctionSymbol
    ): FirFunctionCall {
        TODO("Not yet implemented")
    }
}