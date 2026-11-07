package dev.satex.intellij

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.openapi.options.Configurable
import com.intellij.openapi.ui.TextFieldWithBrowseButton
import com.intellij.ui.components.JBTextField
import com.intellij.util.ui.FormBuilder
import javax.swing.JComponent
import javax.swing.JPanel

@Service(Service.Level.APP)
@State(name = "SatexSettings", storages = [Storage("satex.xml")])
class SatexSettings : PersistentStateComponent<SatexSettings.Data> {
    /** Mirrors satex.path and satex.server.maxMemory of the VS Code extension. */
    data class Data(var path: String = "", var maxMemory: String = "2GiB")

    private var data = Data()
    override fun getState() = data
    override fun loadState(state: Data) { data = state }

    companion object {
        fun get(): SatexSettings = ApplicationManager.getApplication().getService(SatexSettings::class.java)
    }
}

class SatexConfigurable : Configurable {
    private val path = TextFieldWithBrowseButton().apply { toolTipText = "Empty: satex on the PATH" }
    private val memory = JBTextField()
    private var panel: JPanel? = null

    override fun getDisplayName() = "SaTeX"

    override fun createComponent(): JComponent {
        val p = FormBuilder.createFormBuilder()
            .addLabeledComponent("Path to satex (empty: PATH):", path)
            .addLabeledComponent("Memory limit of the server (empty: satex default):", memory)
            .addComponentFillVertically(JPanel(), 0).panel
        panel = p
        return p
    }

    override fun isModified(): Boolean {
        val s = SatexSettings.get().state
        return path.text != s.path || memory.text != s.maxMemory
    }

    override fun apply() {
        val s = SatexSettings.get().state
        s.path = path.text.trim()
        s.maxMemory = memory.text.trim()
    }

    override fun reset() {
        val s = SatexSettings.get().state
        path.text = s.path
        memory.text = s.maxMemory
    }
}
