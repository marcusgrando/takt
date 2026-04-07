import SwiftUI

struct TemplateGridView: View {
    var onSelect: (ActionTemplate) -> Void
    var onBack: () -> Void

    private let columns = [
        GridItem(.flexible(), spacing: 8),
        GridItem(.flexible(), spacing: 8),
    ]

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Button("Back") { onBack() }
                    .buttonStyle(.borderless)
                    .font(.system(size: 13))
                Spacer()
                Text("New Task")
                    .font(.system(size: 13, weight: .semibold))
                Spacer()
                Color.clear.frame(width: 40)
            }
            .padding(.horizontal, 16)
            .frame(height: 40)

            // Grid
            ScrollView {
                LazyVGrid(columns: columns, spacing: 8) {
                    ForEach(ActionTemplate.allCases, id: \.self) { template in
                        Button {
                            onSelect(template)
                        } label: {
                            VStack(spacing: 8) {
                                Image(systemName: template.systemImage)
                                    .font(.system(size: 20))
                                    .foregroundStyle(.secondary)
                                Text(template.label)
                                    .font(.system(size: 13, weight: .medium))
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 16)
                            .background(Color(nsColor: .controlBackgroundColor))
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                            .overlay(
                                RoundedRectangle(cornerRadius: 8)
                                    .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                            )
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
        }
    }
}
