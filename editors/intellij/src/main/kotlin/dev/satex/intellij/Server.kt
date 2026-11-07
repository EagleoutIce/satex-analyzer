package dev.satex.intellij

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider

class SatexServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project): StreamConnectionProvider = SatexConnection(project)
}

/** Runs `satex lsp` on stdio, like the VS Code extension does. */
class SatexConnection(project: Project) : ProcessStreamConnectionProvider() {
    init {
        val s = SatexSettings.get().state
        val binary = s.path.ifBlank { "satex" }
        val args = mutableListOf(binary, "lsp")
        if (s.maxMemory.isNotBlank()) args += listOf("--max-memory", s.maxMemory)
        commands = args
        setWorkingDirectory(project.basePath)
    }
}
