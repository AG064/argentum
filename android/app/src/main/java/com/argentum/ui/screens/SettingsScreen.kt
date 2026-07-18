package com.argentum.ui.screens

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AttachFile
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.CloudDownload
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.DarkMode
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.LightMode
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.OpenInBrowser
import androidx.compose.material.icons.filled.SystemUpdate
import androidx.compose.material.icons.filled.Terminal
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.argentum.BuildConfig
import com.argentum.ui.components.GlassCard
import com.argentum.ui.theme.CrimsonRed
import com.argentum.ui.theme.Silver
import com.argentum.viewmodel.SettingsViewModel
import com.argentum.viewmodel.UpdateState

val PROVIDER_OPTIONS = listOf(
    "minimax" to "MiniMax",
    "openai" to "OpenAI",
    "local" to "Local (llama.cpp)"
)

@Composable
fun SettingsScreen(
    viewModel: SettingsViewModel,
    modifier: Modifier = Modifier
) {
    val uiState by viewModel.uiState.collectAsState()
    val haptic = LocalHapticFeedback.current
    val clipboardManager = LocalClipboardManager.current

    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp)
    ) {
        Text(
            text = "Settings",
            style = MaterialTheme.typography.headlineMedium,
            color = MaterialTheme.colorScheme.onBackground,
            modifier = Modifier.padding(bottom = 16.dp)
        )

        LazyColumn(
            verticalArrangement = Arrangement.spacedBy(8.dp),
            contentPadding = PaddingValues(vertical = 8.dp)
        ) {
            // AI Provider section
            item {
                SectionHeader(title = "AI Provider")
            }

            item {
                ProviderSelector(
                    selectedProvider = uiState.selectedProvider,
                    onProviderSelected = { provider ->
                        haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                        viewModel.selectProvider(provider)
                    }
                )
            }

            // API Configuration section
            item {
                Spacer(modifier = Modifier.height(8.dp))
                SectionHeader(title = "API Configuration")
            }

            item {
                ApiKeyInput(
                    apiKey = uiState.apiKey,
                    onApiKeyChange = { viewModel.updateApiKey(it) }
                )
            }

            item {
                ApiEndpointInput(
                    endpoint = uiState.apiEndpoint,
                    onEndpointChange = { viewModel.updateApiEndpoint(it) }
                )
            }

            // AI Model section
            item {
                Spacer(modifier = Modifier.height(8.dp))
                SectionHeader(title = "AI Model")
            }

            item {
                ModelSelector(
                    selectedModel = uiState.selectedModel,
                    availableModels = uiState.availableModels,
                    onModelSelected = { model ->
                        haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                        viewModel.selectModel(model)
                    }
                )
            }

            // System Prompt section
            item {
                Spacer(modifier = Modifier.height(8.dp))
                SectionHeader(title = "System Prompt")
            }

            item {
                SystemPromptInput(
                    systemPrompt = uiState.systemPrompt,
                    onSystemPromptChange = { viewModel.updateSystemPrompt(it) }
                )
            }

            // Local Server section
            item {
                Spacer(modifier = Modifier.height(8.dp))
                SectionHeader(title = "Local Intelligence")
            }

            item {
                LocalServerInput(
                    localServerUrl = uiState.localServerUrl,
                    onLocalServerUrlChange = { viewModel.updateLocalServerUrl(it) }
                )
            }

            // Input Features section
            item {
                Spacer(modifier = Modifier.height(8.dp))
                SectionHeader(title = "Input Features")
            }

            item {
                SettingsToggleItem(
                    icon = Icons.Default.Mic,
                    title = "Voice Input",
                    subtitle = "Enable speech recognition",
                    checked = true, // TODO: connect to actual state
                    onToggle = { haptic.performHapticFeedback(HapticFeedbackType.LongPress) }
                )
            }

            item {
                SettingsToggleItem(
                    icon = Icons.Default.AttachFile,
                    title = "File Attachments",
                    subtitle = "Allow sending files in chat",
                    checked = true, // TODO: connect to actual state
                    onToggle = { haptic.performHapticFeedback(HapticFeedbackType.LongPress) }
                )
            }

            // Appearance section
            item {
                Spacer(modifier = Modifier.height(8.dp))
                SectionHeader(title = "Appearance")
            }

            item {
                SettingsToggleItem(
                    icon = if (uiState.isDarkMode) Icons.Default.DarkMode else Icons.Default.LightMode,
                    title = "Dark Mode",
                    subtitle = if (uiState.isDarkMode) "Dark theme enabled" else "Light theme enabled",
                    checked = uiState.isDarkMode,
                    onToggle = {
                        haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                        viewModel.toggleDarkMode()
                    }
                )
            }

            // Notifications section
            item {
                Spacer(modifier = Modifier.height(8.dp))
                SectionHeader(title = "Notifications")
            }

            item {
                SettingsToggleItem(
                    icon = Icons.Default.Notifications,
                    title = "Push Notifications",
                    subtitle = if (uiState.notificationsEnabled) "Enabled" else "Disabled",
                    checked = uiState.notificationsEnabled,
                    onToggle = {
                        haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                        viewModel.toggleNotifications()
                    }
                )
            }

            // Updates section
            item {
                Spacer(modifier = Modifier.height(8.dp))
                SectionHeader(title = "Updates")
            }

            item {
                UpdatesCard(
                    updateState = uiState.updateState,
                    onCheck = {
                        haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                        viewModel.checkForUpdates()
                    },
                    onDownload = {
                        haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                        viewModel.downloadUpdate()
                    },
                    onDismiss = { viewModel.dismissUpdateState() },
                )
            }

            // About section
            item {
                Spacer(modifier = Modifier.height(8.dp))
                SectionHeader(title = "About")
            }

            item {
                AboutCard()
            }

            item {
                Spacer(modifier = Modifier.height(8.dp))
                LinksCard()
            }
        }
    }
}

@Composable
private fun SectionHeader(
    title: String,
    modifier: Modifier = Modifier
) {
    Text(
        text = title,
        style = MaterialTheme.typography.labelLarge,
        color = CrimsonRed,
        modifier = modifier.padding(vertical = 8.dp)
    )
}

@Composable
private fun ProviderSelector(
    selectedProvider: String,
    onProviderSelected: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    var expanded by remember { mutableStateOf(false) }
    val haptic = LocalHapticFeedback.current

    val providerLabel = PROVIDER_OPTIONS.find { it.first == selectedProvider }?.second ?: "MiniMax"

    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                expanded = true
            },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    imageVector = Icons.Default.Terminal,
                    contentDescription = null,
                    tint = CrimsonRed
                )

                Spacer(modifier = Modifier.width(16.dp))

                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "Provider",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = providerLabel,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
                    )
                }
            }

            DropdownMenu(
                expanded = expanded,
                onDismissRequest = { expanded = false }
            ) {
                PROVIDER_OPTIONS.forEach { (id, label) ->
                    DropdownMenuItem(
                        text = { Text(label) },
                        onClick = {
                            onProviderSelected(id)
                            expanded = false
                        },
                        trailingIcon = if (id == selectedProvider) {
                            {
                                Icon(
                                    imageVector = Icons.Default.Check,
                                    contentDescription = "Selected",
                                    tint = CrimsonRed
                                )
                            }
                        } else null
                    )
                }
            }
        }
    }
}

@Composable
private fun ApiKeyInput(
    apiKey: String,
    onApiKeyChange: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    var passwordVisible by remember { mutableStateOf(false) }

    Card(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    imageVector = Icons.Default.Key,
                    contentDescription = null,
                    tint = CrimsonRed
                )
                Spacer(modifier = Modifier.width(16.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "API Key",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = "Your API key for selected provider",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
                    )
                }
            }

            Spacer(modifier = Modifier.height(12.dp))

            OutlinedTextField(
                value = apiKey,
                onValueChange = onApiKeyChange,
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("Enter your API key") },
                visualTransformation = if (passwordVisible) VisualTransformation.None else PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = Silver.copy(alpha = 0.5f),
                    unfocusedBorderColor = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f)
                ),
                shape = RoundedCornerShape(8.dp),
                trailingIcon = {
                    TextButton(onClick = { passwordVisible = !passwordVisible }) {
                        Text(
                            text = if (passwordVisible) "Hide" else "Show",
                            color = CrimsonRed
                        )
                    }
                }
            )
        }
    }
}

@Composable
private fun ApiEndpointInput(
    endpoint: String,
    onEndpointChange: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    imageVector = Icons.Default.Link,
                    contentDescription = null,
                    tint = CrimsonRed
                )
                Spacer(modifier = Modifier.width(16.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "API Endpoint",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = "API server URL",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
                    )
                }
            }

            Spacer(modifier = Modifier.height(12.dp))

            OutlinedTextField(
                value = endpoint,
                onValueChange = onEndpointChange,
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("https://api.minimax.io") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = Silver.copy(alpha = 0.5f),
                    unfocusedBorderColor = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f)
                ),
                shape = RoundedCornerShape(8.dp),
                singleLine = true
            )
        }
    }
}

@Composable
private fun SettingsToggleItem(
    icon: ImageVector,
    title: String,
    subtitle: String,
    checked: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable { onToggle() },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = if (checked) CrimsonRed else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
            )

            Spacer(modifier = Modifier.width(16.dp))

            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurface
                )
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
                )
            }

            Switch(
                checked = checked,
                onCheckedChange = { onToggle() },
                colors = SwitchDefaults.colors(
                    checkedThumbColor = CrimsonRed,
                    checkedTrackColor = CrimsonRed.copy(alpha = 0.5f)
                )
            )
        }
    }
}

@Composable
private fun ModelSelector(
    selectedModel: String,
    availableModels: List<String>,
    onModelSelected: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    var expanded by remember { mutableStateOf(false) }
    val haptic = LocalHapticFeedback.current

    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                expanded = true
            },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    imageVector = Icons.Default.Code,
                    contentDescription = null,
                    tint = CrimsonRed
                )

                Spacer(modifier = Modifier.width(16.dp))

                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "AI Model",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = selectedModel,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
                    )
                }
            }

            DropdownMenu(
                expanded = expanded,
                onDismissRequest = { expanded = false }
            ) {
                availableModels.forEach { model ->
                    DropdownMenuItem(
                        text = { Text(model) },
                        onClick = {
                            onModelSelected(model)
                            expanded = false
                        },
                        trailingIcon = if (model == selectedModel) {
                            {
                                Icon(
                                    imageVector = Icons.Default.Check,
                                    contentDescription = "Selected",
                                    tint = CrimsonRed
                                )
                            }
                        } else null
                    )
                }
            }
        }
    }
}

@Composable
private fun SystemPromptInput(
    systemPrompt: String,
    onSystemPromptChange: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    imageVector = Icons.Default.Terminal,
                    contentDescription = null,
                    tint = CrimsonRed
                )
                Spacer(modifier = Modifier.width(16.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "System Prompt",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = "Custom instructions for the AI",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
                    )
                }
            }

            Spacer(modifier = Modifier.height(12.dp))

            OutlinedTextField(
                value = systemPrompt,
                onValueChange = onSystemPromptChange,
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("You are a helpful AI assistant...") },
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = Silver.copy(alpha = 0.5f),
                    unfocusedBorderColor = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f)
                ),
                shape = RoundedCornerShape(8.dp),
                minLines = 3,
                maxLines = 6
            )
        }
    }
}

@Composable
private fun LocalServerInput(
    localServerUrl: String,
    onLocalServerUrlChange: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    imageVector = Icons.Default.Terminal,
                    contentDescription = null,
                    tint = CrimsonRed
                )
                Spacer(modifier = Modifier.width(16.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "Local llama.cpp Server",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = "URL for local AI inference server",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
                    )
                }
            }

            Spacer(modifier = Modifier.height(12.dp))

            OutlinedTextField(
                value = localServerUrl,
                onValueChange = onLocalServerUrlChange,
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("http://127.0.0.1:8080/v1") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = Silver.copy(alpha = 0.5f),
                    unfocusedBorderColor = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f)
                ),
                shape = RoundedCornerShape(8.dp),
                singleLine = true
            )
        }
    }
}

@Composable
private fun UpdatesCard(
    updateState: UpdateState,
    onCheck: () -> Unit,
    onDownload: () -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current

    // When we have a ReadyToInstall state, fire the system installer intent once.
    LaunchedEffect(updateState) {
        val s = updateState
        if (s is UpdateState.ReadyToInstall) {
            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(
                    s.apkUri,
                    "application/vnd.android.package-archive",
                )
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            try {
                context.startActivity(intent)
            } catch (_: ActivityNotFoundException) {
                // No system installer; fall back to opening the release page
                runCatching {
                    context.startActivity(
                        Intent(
                            Intent.ACTION_VIEW,
                            Uri.parse(BuildConfig.GITHUB_RELEASES_PAGE),
                        )
                    )
                }
            }
            onDismiss()
        }
    }

    GlassCard(
        modifier = modifier.fillMaxWidth(),
        cornerRadius = 18.dp,
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    imageVector = Icons.Default.SystemUpdate,
                    contentDescription = null,
                    tint = CrimsonRed,
                )
                Spacer(modifier = Modifier.width(12.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "Argentum updates",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Text(
                        text = "Current version: ${BuildConfig.VERSION_NAME}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                    )
                }
            }

            Spacer(modifier = Modifier.height(12.dp))

            // Status block — one of: Idle, Checking, UpToDate, Available, Downloading, Error
            when (updateState) {
                is UpdateState.Idle -> {
                    Text(
                        text = "Tap below to check for a new release on GitHub.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                    )
                }
                is UpdateState.Checking -> {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(16.dp),
                            strokeWidth = 2.dp,
                            color = CrimsonRed,
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            text = "Checking GitHub for a new release…",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                        )
                    }
                }
                is UpdateState.UpToDate -> {
                    Text(
                        text = "You're on the latest version.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                    )
                }
                is UpdateState.Available -> {
                    Text(
                        text = "Argentum v${updateState.version} is available.",
                        style = MaterialTheme.typography.titleSmall,
                        color = CrimsonRed,
                    )
                    Spacer(modifier = Modifier.height(4.dp))
                    Text(
                        text = "Size: ${humanSize(updateState.apkSize)}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        androidx.compose.material3.Button(
                            onClick = onDownload,
                            colors = androidx.compose.material3.ButtonDefaults.buttonColors(
                                containerColor = CrimsonRed,
                            ),
                            shape = RoundedCornerShape(12.dp),
                        ) {
                            Icon(
                                imageVector = Icons.Default.CloudDownload,
                                contentDescription = null,
                                modifier = Modifier.size(18.dp),
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Download")
                        }
                        Spacer(modifier = Modifier.width(8.dp))
                        androidx.compose.material3.OutlinedButton(
                            onClick = {
                                runCatching {
                                    context.startActivity(
                                        Intent(
                                            Intent.ACTION_VIEW,
                                            Uri.parse(updateState.releasePageUrl),
                                        )
                                    )
                                }
                            },
                            shape = RoundedCornerShape(12.dp),
                        ) {
                            Icon(
                                imageVector = Icons.Default.OpenInBrowser,
                                contentDescription = null,
                                modifier = Modifier.size(18.dp),
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Open in browser")
                        }
                    }
                }
                is UpdateState.Downloading -> {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(16.dp),
                            strokeWidth = 2.dp,
                            color = CrimsonRed,
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            text = "Downloading v${updateState.version}…",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                        )
                    }
                }
                is UpdateState.ReadyToInstall -> {
                    // LaunchedEffect above fires the installer intent and dismisses.
                    Text(
                        text = "Opening the system installer for v${updateState.version}…",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                    )
                }
                is UpdateState.Error -> {
                    Text(
                        text = "Couldn't check for updates: ${updateState.message}",
                        style = MaterialTheme.typography.bodySmall,
                        color = CrimsonRed,
                    )
                }
            }

            Spacer(modifier = Modifier.height(12.dp))

            // Primary action button row
            Row(verticalAlignment = Alignment.CenterVertically) {
                androidx.compose.material3.TextButton(
                    onClick = onCheck,
                    enabled = updateState !is UpdateState.Checking,
                ) {
                    Text(if (updateState is UpdateState.Available) "Re-check" else "Check for updates")
                }
                AnimatedVisibility(visible = updateState is UpdateState.Available ||
                    updateState is UpdateState.UpToDate ||
                    updateState is UpdateState.Error) {
                    TextButton(onClick = onDismiss) {
                        Text("Dismiss")
                    }
                }
            }
        }
    }
}

private fun humanSize(bytes: Long): String {
    if (bytes <= 0) return "—"
    val mb = bytes.toDouble() / (1024.0 * 1024.0)
    return if (mb < 1) "${(bytes / 1024.0).toInt()} KB" else String.format("%.1f MB", mb)
}

@Composable
private fun AboutCard(
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Text(
                text = "Argentum",
                style = MaterialTheme.typography.titleLarge,
                color = MaterialTheme.colorScheme.onSurface
            )
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = "Version ${BuildConfig.VERSION_NAME}",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
            )
            Spacer(modifier = Modifier.height(8.dp))
            Text(
                text = "AI Agent Framework for modern applications",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f)
            )
        }
    }
}

@Composable
private fun LinksCard(
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Text(
                text = "Links",
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.onSurface
            )
            Spacer(modifier = Modifier.height(12.dp))
            
            LinkItem(
                title = "GitHub Repository",
                url = "https://github.com/AG064/argentum"
            )
            
            Spacer(modifier = Modifier.height(8.dp))
            
            LinkItem(
                title = "Report a Bug",
                url = "https://github.com/AG064/argentum/issues/new"
            )
        }
    }
}

@Composable
private fun LinkItem(
    title: String,
    url: String,
    modifier: Modifier = Modifier
) {
    val clipboardManager = LocalClipboardManager.current
    
    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable { 
                clipboardManager.setText(AnnotatedString(url))
            },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f)
        ),
        shape = RoundedCornerShape(8.dp)
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodyMedium,
                color = CrimsonRed
            )
            Text(
                text = url,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
        }
    }
}
